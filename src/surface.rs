//! Shared document-coordinate diff surface. The editors keep their real text;
//! connectors and scroll anchors never introduce padding into saved documents.
use crate::folding::{EqualFold, equal_folds};
use gpui_kit::component::{
    Disableable, Sizable,
    button::{Button, ButtonVariants},
    input::{Editor, EditorState, FoldRange, RopeExt, TextDecoration, TextDecorationCollection},
};
use gpui_kit::*;
use std::time::Duration;
use std::{cell::Cell, collections::HashSet, ops::Range, rc::Rc};

pub const BACKGROUND: u32 = 0x1e1f22;
pub const PANEL: u32 = 0x2b2d30;
pub const BORDER: u32 = 0x393b40;
pub const MODIFIED: u32 = 0x5081a8;
pub const INSERTED: u32 = 0x53966b;
pub const DELETED: u32 = 0x888888;
pub const CONFLICT: u32 = 0xc79750;
const ACTION_WIDTH: f32 = 24.;
const CONNECTOR_WIDTH: f32 = 32.;
const OVERVIEW_WIDTH: f32 = 12.;

pub fn line_ending(text: &str) -> &'static str {
    let mut kinds = 0u8;
    let mut bytes = text.bytes().peekable();
    while let Some(byte) = bytes.next() {
        match byte {
            b'\r' if bytes.peek() == Some(&b'\n') => {
                bytes.next();
                kinds |= 2;
            }
            b'\r' => kinds |= 4,
            b'\n' => kinds |= 1,
            _ => {}
        }
    }
    match kinds {
        0 => "No EOL",
        1 => "LF",
        2 => "CRLF",
        4 => "CR",
        _ => "Mixed",
    }
}

pub fn line_ending_badge(text: &str) -> impl IntoElement + use<> {
    div()
        .flex_shrink_0()
        .px_2()
        .text_xs()
        .text_color(rgb(0x92979f))
        .child(line_ending(text))
}

/// Compact toolbar glyphs: inward chevrons for folding; paired vertical
/// arrows for synchronized scrolling. Button selection supplies the on state.
pub fn toolbar_icon(collapse: bool) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let mut path = PathBuilder::stroke(px(1.2));
            let mut line = |points: &[(f32, f32)]| {
                for (i, &(x, y)) in points.iter().enumerate() {
                    let p = point(bounds.left() + px(x), bounds.top() + px(y));
                    if i == 0 {
                        path.move_to(p);
                    } else {
                        path.line_to(p);
                    }
                }
            };
            if collapse {
                line(&[(4., 2.), (8., 6.), (12., 2.)]);
                line(&[(4., 14.), (8., 10.), (12., 14.)]);
                line(&[(3., 8.), (13., 8.)]);
            } else {
                line(&[(4., 13.), (4., 3.)]);
                line(&[(1., 6.), (4., 3.), (7., 6.)]);
                line(&[(12., 3.), (12., 13.)]);
                line(&[(9., 10.), (12., 13.), (15., 10.)]);
            }
            if let Ok(path) = path.build() {
                window.paint_path(path, window.text_style().color);
            }
        },
    )
    .w(px(16.))
    .h(px(16.))
}

pub fn configure_theme(cx: &mut App) {
    gpui_kit::component::theme::Theme::update(cx, |theme| {
        theme.mono_font_family = "Cascadia Mono".into();
        theme.mono_font_size = px(13.);
        let style = &mut std::sync::Arc::make_mut(&mut theme.highlight_theme).style;
        style.editor_background = Some(rgb(BACKGROUND).into());
        style.editor_gutter_background = Some(rgb(BACKGROUND).into());
        style.editor_foreground = Some(rgb(0xbcbec4).into());
        style.editor_active_line = Some(rgb(0x26282e).into());
        style.editor_line_number = Some(rgb(0x606366).into());
        style.editor_active_line_number = Some(rgb(0xa4a7ad).into());
    });
}

pub fn language(path: &str) -> &'static str {
    match std::path::Path::new(path)
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "rs" => "rust",
        "toml" => "toml",
        "json" | "jsonc" => "json",
        "md" | "markdown" => "markdown",
        _ => "text",
    }
}

#[derive(Default)]
struct ScrollAnchors {
    left: SharedString,
    right: SharedString,
    points: Vec<(usize, usize)>,
}

#[derive(Clone)]
pub struct Change {
    pub id: usize,
    pub ranges: Vec<Range<usize>>,
    pub color: u32,
    pub resolved: bool,
}

pub enum SurfaceEvent {
    Apply { index: usize, source: usize },
    Select(usize),
}

pub struct DiffSurface {
    editors: Vec<Entity<EditorState>>,
    readonly: Vec<bool>,
    changes: Vec<Change>,
    selected: Option<usize>,
    highlights: Vec<TextDecorationCollection>,
    pub sync_scroll: bool,
    pub curved: bool,
    pub highlighting: bool,
    pub disabled: bool,
    pub collapse_unchanged: bool,
    folds: Vec<EqualFold>,
    expanded_folds: HashSet<u64>,
    fold_texts: Vec<SharedString>,
    last_scroll: Vec<Point<Pixels>>,
    pending_scroll: Vec<Option<Pixels>>,
    frame_pending: bool,
    anchors: Vec<ScrollAnchors>,
    gutter_bounds: Vec<Rc<Cell<Bounds<Pixels>>>>,
    overview_bounds: Vec<Rc<Cell<Bounds<Pixels>>>>,
    pane_bounds: Vec<Rc<Cell<Bounds<Pixels>>>>,
    _observers: Vec<Subscription>,
}

impl EventEmitter<SurfaceEvent> for DiffSurface {}

impl DiffSurface {
    pub fn new(
        editors: Vec<Entity<EditorState>>,
        readonly: Vec<bool>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut observers = Vec::new();
        let mut highlights = Vec::new();
        let central_numbers = editors.len() == 2;
        for editor in &editors {
            editor.update(cx, |state, cx| {
                state.set_soft_wrap(false, window, cx);
                state.set_scroll_bounce(false, cx);
                if central_numbers {
                    state.set_line_number(false, window, cx);
                }
            });
            highlights.push(editor.update(cx, |state, cx| {
                state.create_decorations_collection(Vec::new(), cx)
            }));
            observers.push(cx.observe_in(editor, window, |this, _, window, cx| {
                cx.notify();
                // Editor publishes its latest viewport in paint. Read it on the
                // following frame so scrollbar dragging and keyboard scrolling
                // use the same anchors as mouse-wheel scrolling.
                if this.frame_pending {
                    return;
                }
                this.frame_pending = true;
                let weak = cx.entity().downgrade();
                window.on_next_frame(move |_, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        this.frame_pending = false;
                        this.synchronize(cx);
                        cx.notify();
                    });
                });
            }));
        }
        let count = editors.len();
        Self {
            editors,
            readonly,
            changes: Vec::new(),
            selected: None,
            highlights,
            sync_scroll: true,
            curved: true,
            highlighting: true,
            disabled: false,
            collapse_unchanged: true,
            folds: Vec::new(),
            expanded_folds: HashSet::new(),
            fold_texts: Vec::new(),
            last_scroll: vec![point(px(0.), px(0.)); count],
            pending_scroll: vec![None; count],
            frame_pending: false,
            anchors: (0..count).map(|_| ScrollAnchors::default()).collect(),
            gutter_bounds: (1..count)
                .map(|_| Rc::new(Cell::new(Bounds::default())))
                .collect(),
            overview_bounds: (0..count)
                .map(|_| Rc::new(Cell::new(Bounds::default())))
                .collect(),
            pane_bounds: (0..count)
                .map(|_| Rc::new(Cell::new(Bounds::default())))
                .collect(),
            _observers: observers,
        }
    }

    pub fn gutter_width(&self, cx: &App) -> Pixels {
        if self.editors.len() == 3 {
            return px(48.);
        }
        let lines = self
            .editors
            .iter()
            .map(|editor| line_at(editor.read(cx), editor.read(cx).text().len()) + 1)
            .max()
            .unwrap_or(1);
        let digits = lines.ilog10() + 1;
        let number_width = (12. + digits as f32 * 8.).max(52.);
        px(ACTION_WIDTH + number_width * 2. + CONNECTOR_WIDTH)
    }

    pub fn set_changes(
        &mut self,
        changes: Vec<Change>,
        selected: Option<usize>,
        inline: Vec<Vec<Range<usize>>>,
        cx: &mut Context<Self>,
    ) {
        self.changes = changes;
        self.selected = selected;
        let result = self.editors[1].read(cx).value();
        for (index, editor) in self.editors.iter().enumerate() {
            if index == 1 {
                continue;
            }
            let text = editor.read(cx).value();
            let cache = &mut self.anchors[index];
            if cache.left != text || cache.right != result || cache.points.is_empty() {
                cache.points = line_anchors(&text, &result);
                cache.left = text;
                cache.right = result.clone();
            }
        }
        for (pane, collection) in self.highlights.iter().enumerate() {
            collection.set(
                inline
                    .get(pane)
                    .into_iter()
                    .flatten()
                    .map(|range| {
                        TextDecoration::new(
                            range.clone(),
                            HighlightStyle {
                                background_color: Some(rgb(0x365880).into()),
                                ..Default::default()
                            },
                        )
                    })
                    .collect(),
                cx,
            );
        }
        self.rebuild_folds(cx);
        cx.notify();
    }

    pub fn toggle_unchanged(&mut self, cx: &mut Context<Self>) {
        self.collapse_unchanged = !self.collapse_unchanged;
        self.expanded_folds.clear();
        self.fold_texts.clear();
        if self.collapse_unchanged {
            self.rebuild_folds(cx);
        } else {
            self.folds.clear();
            self.apply_folds(cx);
        }
        cx.notify();
    }

    fn rebuild_folds(&mut self, cx: &mut Context<Self>) {
        if !self.collapse_unchanged {
            return;
        }
        let texts: Vec<_> = self.editors.iter().map(|e| e.read(cx).value()).collect();
        if texts == self.fold_texts {
            return;
        }
        self.folds = equal_folds(&texts.iter().map(|s| s.as_ref()).collect::<Vec<_>>());
        self.fold_texts = texts;
        self.expanded_folds
            .retain(|key| self.folds.iter().any(|f| f.key == *key));
        // Never hide an active editable selection when toggling or recomputing.
        for fold in &self.folds {
            for (pane, editor) in self.editors.iter().enumerate() {
                if self.readonly[pane] {
                    continue;
                }
                let state = editor.read(cx);
                let selection = state.selected_range();
                let start = line_at(state, selection.start);
                let end = line_at(state, selection.end);
                let range = &fold.ranges[pane];
                if start < range.end && end >= range.start {
                    self.expanded_folds.insert(fold.key);
                }
            }
        }
        self.apply_folds(cx);
    }

    fn apply_folds(&mut self, cx: &mut Context<Self>) {
        for (pane, editor) in self.editors.iter().enumerate() {
            let folds = self.collapse_unchanged.then(|| {
                self.folds
                    .iter()
                    .filter(|f| !self.expanded_folds.contains(&f.key))
                    .map(|f| FoldRange::new(f.ranges[pane].start, f.ranges[pane].end))
                    .collect()
            });
            editor.update(cx, |state, cx| {
                // Preserve the top buffer line as display rows are removed or
                // restored. Each pane retains its own viewport with sync off.
                let h = state.line_height().unwrap_or(px(20.));
                let offset = state.scroll_offset();
                let row = (-f32::from(offset.y) / f32::from(h)).max(0.);
                let count = state.display_map().display_row_count();
                let line = state
                    .display_map()
                    .display_row_to_buffer_line((row as usize).min(count.saturating_sub(1)));
                state.set_external_folds(folds, cx);
                let y = -h * (display_line(state, line) as f32 + row.fract());
                state.set_scroll_offset(point(offset.x, y), cx);
            });
        }
        self.pending_scroll.fill(Some(px(f32::NAN)));
        cx.notify();
    }

    fn expand_fold(&mut self, key: u64, cx: &mut Context<Self>) {
        self.expanded_folds.insert(key);
        self.apply_folds(cx);
    }

    pub fn select(
        &mut self,
        id: usize,
        focus_result: bool,
        window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        let Some(change) = self.changes.iter().find(|change| change.id == id) else {
            return;
        };
        self.selected = Some(id);
        for (editor, range) in self.editors.iter().zip(&change.ranges) {
            editor.update(cx, |state, cx| {
                state.set_selected_range(range.start..range.start, cx);
                // Include common context before the change. In particular, an
                // empty side of a deletion maps to both ends of the other
                // block; scrolling exactly to that boundary can hide it when
                // the editors publish their new offsets in separate frames.
                let row = change_rows(state, range).start.saturating_sub(3);
                let offset = point(
                    state.scroll_offset().x,
                    -state.line_height().unwrap_or(px(20.)) * row as f32,
                );
                state.set_scroll_offset(offset, cx);
            });
        }
        if focus_result && let Some(window) = window {
            self.editors[1].update(cx, |state, cx| state.focus(window, cx));
        }
        // Navigation explicitly positions all panes; don't mistake that for a
        // user scrolling just one of them during the following frame.
        self.pending_scroll.fill(Some(px(f32::NAN)));
        cx.notify();
    }

    fn synchronize(&mut self, cx: &mut Context<Self>) {
        // Editor navigation/search and edits can open a native fold. Open the
        // matching ranges in the other panes too, without touching their text.
        // set_value (merge/Undo) resets native folds before the asynchronous
        // comparison arrives. That is not an explicit request to expand them.
        let same_text = self
            .editors
            .iter()
            .map(|e| e.read(cx).value())
            .eq(self.fold_texts.iter().cloned());
        let opened: Vec<_> =
            self.folds
                .iter()
                .filter(|_| same_text)
                .filter(|f| !self.expanded_folds.contains(&f.key))
                .filter(|f| {
                    self.editors.iter().enumerate().any(|(pane, e)| {
                        !e.read(cx).display_map().is_folded_at(f.ranges[pane].start)
                    })
                })
                .map(|f| f.key)
                .collect();
        if !opened.is_empty() {
            self.expanded_folds.extend(opened);
            self.apply_folds(cx);
        }
        let offsets: Vec<_> = self
            .editors
            .iter()
            .map(|editor| editor.read(cx).scroll_offset())
            .collect();
        let mut source = None;
        for (index, offset) in offsets.iter().enumerate() {
            // A requested offset can be clamped at a document boundary. Consume
            // the acknowledgement after paint even when no movement occurred,
            // otherwise the next real user scroll would be ignored.
            let requested = self.pending_scroll[index].take().is_some();
            if !requested && (f32::from(offset.y - self.last_scroll[index].y)).abs() > 0.5 {
                source = Some(index);
            }
        }
        self.last_scroll.clone_from(&offsets);
        if !self.sync_scroll {
            return;
        }
        let Some(source) = source else { return };
        let from = self.editors[source].read(cx);
        let line_height = f32::from(from.line_height().unwrap_or(px(20.)));
        let top_line = -f32::from(offsets[source].y) / line_height;
        let mut updates = Vec::new();
        for (target, offset) in offsets.iter().enumerate() {
            if source == target {
                continue;
            }
            let to = self.editors[target].read(cx);
            let result = self.editors[1].read(cx);
            let result_line = if source == 1 {
                top_line
            } else {
                let anchors: Vec<_> = self.anchors[source]
                    .points
                    .iter()
                    .map(|&(a, b)| (display_line(from, a) as f32, display_line(result, b) as f32))
                    .collect();
                map_line(top_line, &anchors)
            };
            let target_line = if target == 1 {
                result_line
            } else {
                let anchors: Vec<_> = self.anchors[target]
                    .points
                    .iter()
                    .map(|&(a, b)| (display_line(result, b) as f32, display_line(to, a) as f32))
                    .collect();
                map_line(result_line, &anchors)
            };
            let y = px(-target_line * f32::from(to.line_height().unwrap_or(px(20.))));
            if (f32::from(y - offset.y)).abs() > 0.5 {
                self.pending_scroll[target] = Some(y);
                updates.push((target, point(offset.x, y)));
            }
        }
        for (target, offset) in updates {
            self.editors[target].update(cx, |state, cx| state.set_scroll_offset(offset, cx));
        }
    }

    fn pane(&self, index: usize, cx: &Context<Self>) -> impl IntoElement {
        let editor = self.editors[index].clone();
        let paint_editor = editor.clone();
        let changes = self.changes.clone();
        let selected = self.selected;
        let highlighting = self.highlighting;
        let geometry = self.pane_bounds[index].clone();
        let view = cx.entity().downgrade();
        let mut content = div()
            .id(("diff-pane", index))
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .font_family("Cascadia Mono")
            .text_size(px(13.))
            .child(
                Editor::new(&editor)
                    .h_full()
                    .bordered(false)
                    .readonly(self.readonly[index])
                    .disabled(self.disabled)
                    .aria_label(if self.editors.len() == 2 {
                        if index == 0 {
                            "Original text"
                        } else {
                            "Modified text"
                        }
                    } else {
                        match index {
                            0 => "Source",
                            1 => "Result",
                            _ => "Target",
                        }
                    }),
            )
            .child(
                canvas(
                    move |bounds, window, _| {
                        if geometry.replace(bounds) != bounds {
                            let view = view.clone();
                            window.on_next_frame(move |_, cx| {
                                let _ = view.update(cx, |_, cx| cx.notify());
                            });
                        }
                    },
                    move |bounds, _, window, cx| {
                        let state = paint_editor.read(cx);
                        window.with_content_mask(Some(ContentMask { bounds }), |window| {
                            for change in &changes {
                                let (top, bottom) = span_y(state, &change.ranges[index]);
                                let active = selected == Some(change.id);
                                let color = rgb(change.color);
                                if highlighting {
                                    let rectangle = Bounds::from_corners(
                                        point(bounds.left(), top),
                                        point(bounds.right(), bottom.max(top + px(2.))),
                                    );
                                    window.paint_quad(fill(
                                        rectangle,
                                        color.opacity(if active { 0.25 } else { 0.16 }),
                                    ));
                                    for y in [top, bottom] {
                                        window.paint_quad(fill(
                                            Bounds::new(
                                                point(bounds.left(), y),
                                                size(bounds.size.width, px(1.)),
                                            ),
                                            color.opacity(if active { 0.85 } else { 0.4 }),
                                        ));
                                    }
                                }
                            }
                        });
                    },
                )
                .absolute()
                .inset_0()
                .size_full(),
            );
        let bounds = self.pane_bounds[index].get();
        let state = editor.read(cx);
        let h = state.line_height().unwrap_or(px(20.));
        for fold in self
            .folds
            .iter()
            .filter(|f| !self.expanded_folds.contains(&f.key))
        {
            let y = state.input_bounds().top()
                + state.scroll_offset().y
                + h * display_line(state, fold.ranges[index].start) as f32
                - bounds.top();
            if y + h < px(0.) || y >= bounds.size.height {
                continue;
            }
            let key = fold.key;
            content = content.child(
                div()
                    .id(("equal-fold", key))
                    .absolute()
                    .left_0()
                    .top(y)
                    .w_full()
                    .h(h)
                    .bg(rgb(BACKGROUND))
                    .cursor_pointer()
                    .overflow_hidden()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            if !this.disabled {
                                this.expand_fold(key, cx);
                            }
                            cx.stop_propagation();
                        }),
                    )
                    .child(
                        canvas(
                            move |_, _, _| (),
                            move |bounds, _, window, _| {
                                paint_fold_wave(
                                    bounds.left(),
                                    bounds.right(),
                                    bounds.center().y,
                                    bounds.center().y,
                                    window,
                                );
                            },
                        )
                        .absolute()
                        .inset_0()
                        .size_full(),
                    ),
            );
        }
        // Reserve a separate lane so clicking a change never intercepts the
        // editor's scrollbar thumb or covers text. The two outer lanes mirror
        // one another, keeping the line numbers and merge actions in the middle.
        let pane = div().flex().flex_1().min_w_0().h_full();
        if index == 0 {
            pane.child(self.overview(index, cx)).child(content)
        } else {
            pane.child(content).child(self.overview(index, cx))
        }
    }

    fn overview(&self, index: usize, cx: &Context<Self>) -> impl IntoElement {
        let editor = self.editors[index].clone();
        let changes = self.changes.clone();
        let selected = self.selected;
        let geometry = self.overview_bounds[index].clone();
        div()
            .id(("diff-overview", index))
            .relative()
            .w(px(OVERVIEW_WIDTH))
            .flex_shrink_0()
            .h_full()
            .bg(rgb(0x242629))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    if this.disabled {
                        return;
                    }
                    let bounds = this.overview_bounds[index].get();
                    if !bounds.contains(&event.position) || bounds.size.height <= px(0.) {
                        return;
                    }
                    let state = this.editors[index].read(cx);
                    let markers: Vec<_> = this
                        .changes
                        .iter()
                        .map(|change| {
                            (
                                change.id,
                                overview_marker(state, &change.ranges[index], bounds.size.height),
                            )
                        })
                        .collect();
                    if let Some(id) =
                        overview_hit(&markers, f32::from(event.position.y - bounds.top()))
                    {
                        cx.emit(SurfaceEvent::Select(id));
                    }
                    cx.stop_propagation();
                }),
            )
            .child(
                canvas(
                    move |bounds, _, _| geometry.set(bounds),
                    move |bounds, _, window, cx| {
                        let state = editor.read(cx);
                        window.with_content_mask(Some(ContentMask { bounds }), |window| {
                            // The viewport hint moves while the change map stays
                            // anchored to the complete (possibly folded) document.
                            let rows = state.display_map().display_row_count().max(1);
                            let line_height = f32::from(state.line_height().unwrap_or(px(20.)));
                            let first = -f32::from(state.scroll_offset().y) / line_height;
                            let visible = f32::from(state.input_bounds().size.height) / line_height;
                            let top = (first / rows as f32).clamp(0., 1.);
                            let bottom = ((first + visible) / rows as f32).clamp(top, 1.);
                            window.paint_quad(fill(
                                Bounds::from_corners(
                                    point(bounds.left(), bounds.top() + bounds.size.height * top),
                                    point(
                                        bounds.right(),
                                        bounds.top() + bounds.size.height * bottom,
                                    ),
                                ),
                                rgb(0x7a7e85).opacity(0.25),
                            ));
                            // Draw the selected marker last so small adjacent
                            // changes cannot obscure its outline.
                            for change in changes
                                .iter()
                                .filter(|change| selected != Some(change.id))
                                .chain(changes.iter().filter(|change| selected == Some(change.id)))
                            {
                                let span = overview_marker(
                                    state,
                                    &change.ranges[index],
                                    bounds.size.height,
                                );
                                let active = selected == Some(change.id);
                                let top = bounds.top() + px(span.start);
                                let bottom = bounds.top() + px(span.end);
                                let rectangle = Bounds::from_corners(
                                    point(bounds.left() + px(1.), top),
                                    point(bounds.right() - px(1.), bottom),
                                );
                                window.paint_quad(fill(
                                    rectangle,
                                    rgb(change.color).opacity(if active {
                                        1.
                                    } else if change.resolved {
                                        0.45
                                    } else {
                                        0.85
                                    }),
                                ));
                                if active {
                                    for y in [top, bottom - px(1.)] {
                                        window.paint_quad(fill(
                                            Bounds::new(
                                                point(rectangle.left(), y),
                                                size(rectangle.size.width, px(1.)),
                                            ),
                                            rgb(0xd2d6dc).opacity(0.85),
                                        ));
                                    }
                                }
                            }
                        });
                    },
                )
                .size_full(),
            )
    }

    fn gutter(&self, left: usize, cx: &Context<Self>) -> impl IntoElement {
        let right = left + 1;
        let editors = [self.editors[left].clone(), self.editors[right].clone()];
        let changes = self.changes.clone();
        let selected = self.selected;
        let curved = self.curved;
        let highlighting = self.highlighting;
        let collapse = self.collapse_unchanged;
        let folds: Vec<_> = self
            .folds
            .iter()
            .filter(|f| !self.expanded_folds.contains(&f.key))
            .cloned()
            .collect();
        let three_way = self.editors.len() == 3;
        let source = if three_way && left == 1 { 2 } else { 0 };
        let width = self.gutter_width(cx);
        let number_width = (width - px(ACTION_WIDTH + CONNECTOR_WIDTH)) / 2.;
        let geometry = self.gutter_bounds[left].get();
        let origin = geometry.top();
        let height = geometry.size.height;
        let geometry_cache = self.gutter_bounds[left].clone();
        let view = cx.entity().downgrade();
        let mut gutter = div()
            .id(("diff-gutter", left))
            .relative()
            .w(width)
            .h_full()
            .flex_shrink_0()
            .overflow_hidden()
            .bg(rgb(BACKGROUND))
            .border_x_1()
            .border_color(rgb(BORDER))
            .child(
                canvas(
                    move |bounds, window, _| {
                        if geometry_cache.replace(bounds) != bounds {
                            let view = view.clone();
                            window.on_next_frame(move |_, cx| {
                                let _ = view.update(cx, |_, cx| cx.notify());
                            });
                        }
                    },
                    move |bounds, _, window, cx| {
                        window.with_content_mask(Some(ContentMask { bounds }), |window| {
                            for change in changes.iter().filter(|_| highlighting) {
                                let (a, b) = span_y(editors[0].read(cx), &change.ranges[left]);
                                let (c, d) = span_y(editors[1].read(cx), &change.ranges[right]);
                                if b.max(d) < bounds.top() || a.min(c) > bounds.bottom() {
                                    continue;
                                }
                                let mut path = PathBuilder::fill();
                                // Keep each line-number column level with its
                                // document. Bend only between the two columns.
                                let (x1, x2) = if three_way {
                                    (bounds.left(), bounds.right())
                                } else {
                                    let x1 = bounds.left() + px(ACTION_WIDTH) + number_width;
                                    (x1, x1 + px(CONNECTOR_WIDTH))
                                };
                                let mid = (x1 + x2) / 2.;
                                path.move_to(point(bounds.left(), a));
                                path.line_to(point(x1, a));
                                if curved {
                                    path.cubic_bezier_to(
                                        point(x2, c),
                                        point(mid, a),
                                        point(mid, c),
                                    );
                                } else {
                                    path.line_to(point(x2, c));
                                }
                                path.line_to(point(bounds.right(), c));
                                path.line_to(point(bounds.right(), d.max(c + px(1.))));
                                path.line_to(point(x2, d.max(c + px(1.))));
                                if curved {
                                    path.cubic_bezier_to(
                                        point(x1, b.max(a + px(1.))),
                                        point(mid, d),
                                        point(mid, b),
                                    );
                                } else {
                                    path.line_to(point(x1, b.max(a + px(1.))));
                                }
                                path.line_to(point(bounds.left(), b.max(a + px(1.))));
                                path.close();
                                if let Ok(path) = path.build() {
                                    window.paint_path(
                                        path,
                                        rgb(change.color).opacity(if selected == Some(change.id) {
                                            0.38
                                        } else {
                                            0.23
                                        }),
                                    );
                                }
                            }
                            if !three_way {
                                for (pane, editor) in editors.iter().enumerate() {
                                    let right_edge = if pane == 0 {
                                        bounds.left() + px(ACTION_WIDTH) + number_width
                                    } else {
                                        bounds.right()
                                    };
                                    paint_line_numbers(
                                        editor,
                                        bounds,
                                        right_edge - px(8.),
                                        collapse,
                                        window,
                                        cx,
                                    );
                                }
                            }
                            for fold in &folds {
                                let y = |pane: usize| {
                                    let state = editors[pane].read(cx);
                                    let h = state.line_height().unwrap_or(px(20.));
                                    state.input_bounds().top()
                                        + state.scroll_offset().y
                                        + h * (display_line(state, fold.ranges[left + pane].start)
                                            as f32
                                            + 0.5)
                                };
                                paint_fold_wave(bounds.left(), bounds.right(), y(0), y(1), window);
                            }
                        });
                    },
                )
                .absolute()
                .inset_0()
                .size_full(),
            );
        for change in &self.changes {
            let source_state = self.editors[source].read(cx);
            let (y, _) = span_y(source_state, &change.ranges[source]);
            let y = y - origin;
            if y < px(-1.) || y > height - px(16.) {
                continue;
            }
            let id = change.id;
            gutter = gutter.child(
                div()
                    .absolute()
                    .top(y)
                    .left(px(if source == 2 { 22. } else { 1. }))
                    .child(
                        Button::new(("apply-block", id))
                            .ghost()
                            .xsmall()
                            .w(px(24.))
                            .h(px(20.))
                            .child(merge_chevrons(source == 2))
                            .tooltip(if change.resolved && three_way {
                                "Replace the resolved block with this side again"
                            } else if source == 2 {
                                "Apply this Target block to Result"
                            } else {
                                "Apply this block to the right"
                            })
                            .disabled(self.disabled)
                            .on_click(cx.listener(move |_, _, _, cx| {
                                cx.emit(SurfaceEvent::Apply { index: id, source })
                            })),
                    ),
            );
        }
        gutter
    }
}

fn paint_line_numbers(
    editor: &Entity<EditorState>,
    bounds: Bounds<Pixels>,
    right_edge: Pixels,
    hide_fold_headers: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let (height, numbers) = {
        let editor = editor.read(cx);
        let height = editor.line_height().unwrap_or(px(20.));
        let origin = editor.input_bounds().top() + editor.scroll_offset().y;
        let count = editor.display_map().display_row_count();
        let first = (f32::from(bounds.top() - origin) / f32::from(height))
            .floor()
            .max(0.) as usize;
        let end = ((f32::from(bounds.bottom() - origin) / f32::from(height))
            .ceil()
            .max(0.) as usize)
            .min(count);
        let current = line_at(editor, editor.selected_range().start);
        let numbers: Vec<_> = (first..end)
            .map(|row| {
                let line = editor.display_map().display_row_to_buffer_line(row);
                (line, origin + height * row as f32, line == current)
            })
            .filter(|(line, _, _)| !hide_fold_headers || !editor.display_map().is_folded_at(*line))
            .collect();
        (height, numbers)
    };
    for (buffer_line, y, active) in numbers {
        let text: SharedString = (buffer_line + 1).to_string().into();
        let line = window.text_system().shape_line(
            text.clone(),
            px(13.),
            &[TextRun {
                len: text.len(),
                font: font("Cascadia Mono"),
                color: rgb(if active { 0xa4a7ad } else { 0x606a79 }).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }],
            None,
        );
        let _ = line.paint(
            point(right_edge - line.width, y),
            height,
            TextAlign::Left,
            None,
            window,
            cx,
        );
    }
}

/// Font-independent, thin double chevron matching the editor gutter controls.
fn merge_chevrons(left: bool) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let mut path = PathBuilder::stroke(px(1.));
            for x in [3., 8.] {
                let p = |x: f32, y: f32| {
                    point(
                        bounds.left() + px(if left { 16. - x } else { x }),
                        bounds.top() + px(y),
                    )
                };
                path.move_to(p(x, 3.5));
                path.line_to(p(x + 4., 8.));
                path.line_to(p(x, 12.5));
            }
            if let Ok(path) = path.build() {
                window.paint_path(path, window.text_style().color);
            }
        },
    )
    .w(px(16.))
    .h(px(16.))
}

fn paint_fold_wave(
    left: Pixels,
    right: Pixels,
    start_y: Pixels,
    end_y: Pixels,
    window: &mut Window,
) {
    let width = f32::from(right - left);
    if width <= 0. {
        return;
    }
    // Use screen coordinates for the phase so the wave stays continuous
    // across editor/gutter boundaries and does not stretch when panes resize.
    let steps = width.ceil() as usize;
    let mut path = PathBuilder::stroke(px(1.));
    for n in 0..=steps {
        let t = n as f32 / steps as f32;
        let x = left + (right - left) * t;
        let bend = t * t * (3. - 2. * t);
        let wave = (f32::from(x) * std::f32::consts::TAU / 14.).sin() * 2.;
        let p = point(x, start_y + (end_y - start_y) * bend + px(wave));
        if n == 0 {
            path.move_to(p);
        } else {
            path.line_to(p);
        }
    }
    if let Ok(path) = path.build() {
        window.paint_path(path, rgb(0x55585e));
    }
}

impl Render for DiffSurface {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut row = div().flex().size_full().min_h_0().bg(rgb(BACKGROUND));
        for index in 0..self.editors.len() {
            if index > 0 {
                row = row.child(self.gutter(index - 1, cx));
            }
            row = row.child(self.pane(index, cx));
        }
        row
    }
}

fn line_at(editor: &EditorState, byte: usize) -> usize {
    editor
        .text()
        .offset_to_position(byte.min(editor.text().len()))
        .line as usize
}

fn display_line(editor: &EditorState, line: usize) -> usize {
    if line > line_at(editor, editor.text().len()) {
        editor.display_map().display_row_count()
    } else {
        editor.display_map().buffer_line_to_display_row(line)
    }
}

fn span_y(editor: &EditorState, range: &Range<usize>) -> (Pixels, Pixels) {
    let h = editor.line_height().unwrap_or(px(20.));
    let origin = editor.input_bounds().top() + editor.scroll_offset().y;
    let rows = change_rows(editor, range);
    (origin + h * rows.start as f32, origin + h * rows.end as f32)
}

fn change_rows(editor: &EditorState, range: &Range<usize>) -> Range<usize> {
    let first = line_at(editor, range.start);
    let end_line = line_at(editor, range.end);
    let end = if range.is_empty() {
        first
    } else if editor
        .text()
        .offset_to_position(range.end.min(editor.text().len()))
        .character
        > 0
    {
        end_line + 1
    } else {
        end_line.max(first + 1)
    };
    display_line(editor, first)..display_line(editor, end)
}

fn overview_marker(editor: &EditorState, range: &Range<usize>, height: Pixels) -> Range<f32> {
    overview_span(
        change_rows(editor, range),
        editor.display_map().display_row_count(),
        f32::from(height),
        f32::from(editor.line_height().unwrap_or(px(20.))),
    )
}

/// Project a change onto the full file map, keeping empty-side and EOF changes
/// visible and clickable without painting beyond the track.
fn overview_span(rows: Range<usize>, total: usize, height: f32, line_height: f32) -> Range<f32> {
    let height = height.max(0.);
    // Short files occupy the top of the track, just like the editor viewport;
    // a single changed line must not expand to a large fraction of the window.
    let scale = (height / total.max(1) as f32).min(line_height.max(1.));
    let start = rows.start.min(total) as f32 * scale;
    let end = rows.end.min(total) as f32 * scale;
    let length = (end - start).max(3.).min(height);
    let top = start.min(height - length);
    top..top + length
}

fn overview_hit(markers: &[(usize, Range<f32>)], y: f32) -> Option<usize> {
    markers
        .iter()
        .min_by(|(_, a), (_, b)| {
            let distance = |span: &Range<f32>| (span.start - y).max(y - span.end).max(0.);
            let center_distance = |span: &Range<f32>| ((span.start + span.end) / 2. - y).abs();
            distance(a)
                .total_cmp(&distance(b))
                .then_with(|| center_distance(a).total_cmp(&center_distance(b)))
        })
        .map(|(id, _)| *id)
}

fn line_anchors(left: &str, right: &str) -> Vec<(usize, usize)> {
    let diff = similar::TextDiff::configure()
        .timeout(Duration::from_millis(20))
        .diff_lines(left, right);
    let mut anchors = vec![(0, 0)];
    for op in diff.ops() {
        anchors.push((op.old_range().start, op.new_range().start));
        anchors.push((op.old_range().end, op.new_range().end));
    }
    anchors
}

/// Piecewise line mapping between unchanged spans and unequal change blocks.
pub fn map_line(line: f32, anchors: &[(f32, f32)]) -> f32 {
    let mut previous = (0., 0.);
    for &(a, b) in anchors {
        if a < previous.0 || b < previous.1 {
            continue;
        }
        if a > line && a > previous.0 {
            return previous.1 + (line - previous.0) / (a - previous.0) * (b - previous.1);
        }
        previous = (a, b);
    }
    previous.1 + (line - previous.0).max(0.)
}

#[cfg(test)]
mod tests {
    #[test]
    fn detects_line_endings_without_confusing_crlf_with_mixed() {
        assert_eq!(super::line_ending(""), "No EOL");
        assert_eq!(super::line_ending("한글"), "No EOL");
        assert_eq!(super::line_ending("a\nb"), "LF");
        assert_eq!(super::line_ending("a\r\nb\r\n"), "CRLF");
        assert_eq!(super::line_ending("a\rb"), "CR");
        assert_eq!(super::line_ending("a\r\nb\nc"), "Mixed");
        assert_eq!(super::line_ending("a\rb\n"), "Mixed");
    }
    use super::{line_anchors, map_line, overview_hit, overview_span};

    #[test]
    fn overview_projects_each_documents_change_extent() {
        assert_eq!(overview_span(20..40, 100, 200., 20.), 40.0..80.0);
        assert_eq!(overview_span(20..40, 200, 200., 20.), 20.0..40.0);
        assert_eq!(overview_span(20..20, 100, 200., 20.), 40.0..43.0);
        assert_eq!(overview_span(1..2, 9, 800., 20.), 20.0..40.0);
    }

    #[test]
    fn overview_keeps_empty_and_eof_changes_inside_short_tracks() {
        assert_eq!(overview_span(100..100, 100, 200., 20.), 197.0..200.0);
        assert_eq!(overview_span(0..0, 0, 200., 20.), 0.0..3.0);
        assert_eq!(overview_span(1..1, 1, 2., 20.), 0.0..2.0);
        assert_eq!(overview_span(1..1, 1, 0., 20.), 0.0..0.0);
    }

    #[test]
    fn overview_click_targets_the_block_extent_and_nearest_visible_marker() {
        let markers = [(7, 20.0..80.0), (9, 85.0..88.0), (12, 197.0..200.0)];
        assert_eq!(overview_hit(&markers, 77.), Some(7));
        assert_eq!(overview_hit(&markers, 87.), Some(9));
        assert_eq!(overview_hit(&markers, 130.), Some(9));
        assert_eq!(overview_hit(&markers, 199.), Some(12));
        assert_eq!(overview_hit(&[], 100.), None);
    }
    #[test]
    fn scroll_mapping_handles_insertions_deletions_and_unequal_replacements() {
        let anchors = [
            (0., 0.),
            (4., 4.),
            (4., 7.),
            (10., 13.),
            (14., 15.),
            (30., 31.),
        ];
        assert_eq!(map_line(2., &anchors), 2.);
        assert_eq!(map_line(4., &anchors), 7.);
        assert_eq!(map_line(7., &anchors), 10.);
        assert_eq!(map_line(12., &anchors), 14.);
        assert_eq!(map_line(40., &anchors), 41.);
        let reverse: Vec<_> = anchors.iter().map(|&(a, b)| (b, a)).collect();
        assert_eq!(map_line(5., &reverse), 4.);
        assert_eq!(map_line(14., &reverse), 12.);
    }

    #[test]
    fn anchors_include_automatic_changes_outside_conflicts() {
        let anchors: Vec<_> = line_anchors("a\nb\nc\nd\n", "a\ninserted\nb\nc\nd\n")
            .into_iter()
            .map(|(a, b)| (a as f32, b as f32))
            .collect();
        assert_eq!(map_line(0., &anchors), 0.);
        assert_eq!(map_line(2., &anchors), 3.);
        assert_eq!(map_line(3.5, &anchors), 4.5);
    }
}
