use crate::{
    diff,
    merge::{self, MergeDocument, MergeInput, MergePaths, Resolution},
    surface::{self, Change, DiffSurface, SurfaceEvent},
};
use crate::{i18n::tr, settings};
use gpui_kit::component::{
    Disableable, Selectable, Sizable,
    button::{Button, ButtonVariants},
    input::{Editor, EditorState, InputEvent},
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::sync::{
    Arc,
    atomic::{AtomicU8, Ordering},
};

const BG: u32 = surface::BACKGROUND;
const PANEL: u32 = surface::PANEL;
const BORDER: u32 = surface::BORDER;
const MUTED: u32 = 0x8491a3;
const TEXT: u32 = 0xd6dfeb;
const GREEN: u32 = 0x83d6ad;
const AMBER: u32 = 0xe2c88a;

struct MergeApp {
    settings: Entity<settings::SettingsPanel>,
    paths: MergePaths,
    input: Option<MergeInput>,
    document: Option<MergeDocument>,
    selected: Option<usize>,
    source: Entity<EditorState>,
    base: Entity<EditorState>,
    target: Entity<EditorState>,
    result: Entity<EditorState>,
    surface: Entity<DiffSurface>,
    _surface_subscription: Subscription,
    show_conflicts: bool,
    highlight: diff::HighlightMode,
    show_base: bool,
    saved_text: Option<String>,
    changed: bool,
    saving: bool,
    close_prompt: bool,
    status: String,
    exit: Arc<AtomicU8>,
    _subscription: Subscription,
}

impl MergeApp {
    fn new(
        paths: MergePaths,
        exit: Arc<AtomicU8>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let source = cx.new(|cx| {
            EditorState::new(window, cx)
                .language(surface::language(&paths.source.to_string_lossy()))
        });
        let target = cx.new(|cx| {
            EditorState::new(window, cx)
                .language(surface::language(&paths.target.to_string_lossy()))
        });
        let base = cx.new(|cx| {
            EditorState::new(window, cx).language(surface::language(&paths.base.to_string_lossy()))
        });
        let result = cx.new(|cx| {
            EditorState::new(window, cx)
                .language(surface::language(&paths.target.to_string_lossy()))
        });
        let surface = cx.new(|cx| {
            DiffSurface::new(
                vec![source.clone(), result.clone(), target.clone()],
                vec![true, false, true],
                window,
                cx,
            )
        });
        let surface_subscription = cx.subscribe_in(
            &surface,
            window,
            |this, _, event: &SurfaceEvent, window, cx| match *event {
                SurfaceEvent::Apply { index, source } => {
                    this.select(index, window, cx);
                    this.resolve(
                        Some(if source == 0 {
                            Resolution::Source
                        } else {
                            Resolution::Target
                        }),
                        window,
                        cx,
                    );
                }
                SurfaceEvent::Select(index) => this.select(index, window, cx),
                SurfaceEvent::DropFiles { .. } => {}
            },
        );
        let subscription = cx.subscribe_in(&result, window, |this, editor, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::Change) && !this.saving {
                let text = editor.read(cx).value().to_string();
                if let Some(doc) = &mut this.document && doc.edit(text) {
                    this.changed = true;
                    this.update_exit();
                    this.paint_conflicts(cx);
                    this.status = "Result edited · Edited conflicts need Mark resolved · Ctrl+Z / Ctrl+Shift+Z".into();
                    cx.notify();
                }
            }
        });
        let load_paths = paths.clone();
        let load = cx.background_executor().spawn(async move {
            let input = MergeInput::load(&load_paths)?;
            let document = MergeDocument::new(&input.base, &input.source, &input.target);
            Ok::<_, String>((input, document))
        });
        cx.spawn_in(window, async move |this, cx| {
            let loaded = load.await;
            let _ = this.update_in(cx, |this, window, cx| {
                match loaded {
                    Ok((input, document)) => {
                        this.source.update(cx, |state, cx| state.set_value(input.source.clone(), window, cx));
                        this.target.update(cx, |state, cx| state.set_value(input.target.clone(), window, cx));
                        this.selected = (!document.conflicts.is_empty()).then_some(0);
                        this.input = Some(input);
                        this.document = Some(document);
                        this.refresh_result(window, cx);
                        this.show_selection(window, cx);
                        this.status = "Edit Result at any time · Source >> / << Target applies the selected conflict · Base starts unresolved blocks".into();
                    }
                    Err(error) => {
                        this.exit.store(2, Ordering::SeqCst);
                        this.status = error;
                    }
                }
                cx.notify();
            });
        }).detach();
        Self {
            settings: cx.new(|cx| settings::SettingsPanel::new(window, cx)),
            paths,
            input: None,
            document: None,
            selected: None,
            source,
            base,
            target,
            result,
            surface,
            _surface_subscription: surface_subscription,
            show_conflicts: false,
            highlight: diff::HighlightMode::Words,
            show_base: false,
            saved_text: None,
            changed: false,
            saving: false,
            close_prompt: false,
            status: "Loading inputs and computing three-way merge…".into(),
            exit,
            _subscription: subscription,
        }
    }

    fn ready(&self) -> bool {
        self.document
            .as_ref()
            .is_some_and(|doc| doc.remaining() == 0)
    }

    fn update_exit(&self) {
        self.exit.store(
            if self.ready()
                && self.document.as_ref().map(MergeDocument::text) == self.saved_text.as_deref()
            {
                0
            } else {
                1
            },
            Ordering::SeqCst,
        );
    }

    fn refresh_result(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(doc) = &self.document {
            let cursor = self
                .result
                .read(cx)
                .selected_range()
                .start
                .min(doc.text().len());
            self.result.update(cx, |state, cx| {
                state.set_value(doc.text().to_owned(), window, cx);
                state.set_selected_range(cursor..cursor, cx);
            });
            self.paint_conflicts(cx);
            self.update_exit();
        }
    }

    fn paint_conflicts(&self, cx: &mut Context<Self>) {
        let Some(doc) = &self.document else { return };
        let changes = doc
            .conflicts
            .iter()
            .enumerate()
            .map(|(id, c)| Change {
                id,
                ranges: vec![
                    c.source_range.clone(),
                    c.result_range.clone(),
                    c.target_range.clone(),
                ],
                color: if c.resolution.is_some() {
                    surface::INSERTED
                } else {
                    surface::CONFLICT
                },
                resolved: c.resolution.is_some(),
            })
            .collect();
        let mut inline = vec![Vec::new(), Vec::new(), Vec::new()];
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(35);
        for conflict in &doc.conflicts {
            if std::time::Instant::now() > deadline {
                break;
            }
            let result = &doc.text()[conflict.result_range.clone()];
            for (pane, text, offset) in [
                (0, conflict.source.as_str(), conflict.source_range.start),
                (2, conflict.target.as_str(), conflict.target_range.start),
            ] {
                let [outer, middle] = diff::inline_changes(text, result, self.highlight);
                inline[pane].extend(outer.into_iter().map(|r| r.start + offset..r.end + offset));
                let offset = conflict.result_range.start;
                inline[1].extend(middle.into_iter().map(|r| r.start + offset..r.end + offset));
            }
        }
        // Both outer panes can highlight the same result token.
        inline[1].sort_by_key(|r| (r.start, r.end));
        inline[1].dedup();
        self.surface.update(cx, |surface, cx| {
            surface.highlighting = self.highlight != diff::HighlightMode::None;
            surface.set_changes(changes, self.selected, inline, cx)
        });
    }
    fn show_selection(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(conflict) = self
            .selected
            .and_then(|i| self.document.as_ref()?.conflicts.get(i))
        else {
            return;
        };
        self.base.update(cx, |state, cx| {
            state.set_value(conflict.base.clone(), window, cx)
        });
        self.surface.update(cx, |surface, cx| {
            surface.select(self.selected.unwrap(), true, Some(window), cx)
        });
        self.paint_conflicts(cx);
    }

    fn select(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving
            || self
                .document
                .as_ref()
                .is_none_or(|doc| index >= doc.conflicts.len())
        {
            return;
        }
        self.selected = Some(index);
        self.show_selection(window, cx);
        cx.notify();
    }

    fn navigate(&mut self, next: bool, window: &mut Window, cx: &mut Context<Self>) {
        let count = self.document.as_ref().map_or(0, |doc| doc.conflicts.len());
        if count == 0 {
            return;
        }
        let current = self.selected.unwrap_or(0);
        self.select(
            if next {
                (current + 1) % count
            } else {
                (current + count - 1) % count
            },
            window,
            cx,
        );
    }

    fn resolve(&mut self, choice: Option<Resolution>, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        let Some(index) = self.selected else { return };
        if let Some(doc) = &mut self.document
            && doc.resolve(index, choice)
        {
            self.after_action(
                "Conflict applied · Other Result edits preserved",
                window,
                cx,
            );
        }
    }

    fn mark_resolved(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        let Some(index) = self.selected else { return };
        if let Some(doc) = &mut self.document
            && doc.mark_resolved(index)
        {
            self.after_action("Current Result block marked resolved", window, cx);
        }
    }

    fn history(&mut self, redo: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        if let Some(doc) = &mut self.document
            && if redo { doc.redo() } else { doc.undo() }
        {
            self.changed = true;
            self.refresh_result(window, cx);
            self.result.update(cx, |state, cx| state.focus(window, cx));
            self.status = if redo {
                "Redo · Result and conflict state restored"
            } else {
                "Undo · Result and conflict state restored"
            }
            .into();
            cx.notify();
        }
    }

    fn after_action(&mut self, status: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.changed = true;
        self.refresh_result(window, cx);
        self.show_selection(window, cx);
        self.status = status.into();
        cx.notify();
    }

    fn save(&mut self, close: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ready() || self.saving {
            return;
        }
        let Some(doc) = &self.document else { return };
        let bom = self.input.as_ref().is_some_and(|input| input.bom);
        if doc.text().len() + if bom { 3 } else { 0 } > diff::MAX_BYTES {
            self.status = "Result exceeds the 2 MiB limit. Save was stopped.".into();
            cx.notify();
            return;
        }
        self.saving = true;
        self.status = format!("Saving {}…", self.paths.result.display());
        let text = doc.text().to_owned();
        let saved = text.clone();
        let path = self.paths.result.clone();
        let task = cx
            .background_executor()
            .spawn(async move { merge::save_result(&path, &text, bom) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.saving = false;
                match result {
                    Ok(()) => {
                        this.saved_text = Some(saved);
                        this.changed = false;
                        this.update_exit();
                        this.status = format!("Saved {}", this.paths.result.display());
                        if close {
                            window.remove_window();
                        }
                    }
                    Err(error) => {
                        this.exit.store(2, Ordering::SeqCst);
                        this.status = error;
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.saving || self.close_prompt {
            return false;
        }
        if !self.changed || self.exit.load(Ordering::SeqCst) == 0 {
            return true;
        }
        self.close_prompt = true;
        let ready = self.ready();
        let answers = if ready {
            vec!["Save", "Discard", "Cancel"]
        } else {
            vec!["Discard", "Cancel"]
        };
        let answers: Vec<String> = answers.into_iter().map(tr).collect();
        let answer_refs: Vec<&str> = answers.iter().map(String::as_str).collect();
        let prompt = window.prompt(
            PromptLevel::Warning,
            &tr("Close with unsaved merge changes?"),
            Some(&tr(if ready {
                "Save Result before closing."
            } else {
                "Resolve all conflicts before saving. Cancel returns to the merge."
            })),
            &answer_refs,
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let answer = prompt.await.ok();
            let _ = this.update_in(cx, |this, window, cx| {
                this.close_prompt = false;
                if ready && answer == Some(0) {
                    this.save(true, window, cx);
                } else if answer == Some(usize::from(ready)) {
                    window.remove_window();
                }
                cx.notify();
            });
        })
        .detach();
        false
    }

    fn pane(
        &self,
        label: &'static str,
        path: String,
        editor: &Entity<EditorState>,
        readonly: bool,
        color: u32,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .h_full()
            .border_r_1()
            .border_color(rgb(settings::color(BORDER)))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .px_3()
                    .py_2()
                    .bg(rgb(settings::color(PANEL)))
                    .flex_shrink_0()
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(settings::color(color)))
                            .child(tr(label)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(settings::color(MUTED)))
                            .overflow_hidden()
                            .text_ellipsis()
                            .child(path),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .font_family("Cascadia Mono")
                    .child(
                        Editor::new(editor)
                            .h_full()
                            .bordered(false)
                            .readonly(readonly)
                            .disabled(self.saving || self.document.is_none())
                            .aria_label(tr(label)),
                    ),
            )
    }
}

impl MergeApp {
    fn comparison_controls(&self, cx: &mut Context<Self>) -> Div {
        let curved = self.surface.read(cx).curved;

        div()
            .w(px(290.))
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                Button::new("highlight")
                    .small()
                    .ghost()
                    .label(format!(
                        "{}: {}",
                        tr("Highlight"),
                        tr(self.highlight.label())
                    ))
                    .accessibility_label(tr("Highlighting: Words / Characters / Lines / None"))
                    .tooltip(tr("Highlighting: Words / Characters / Lines / None"))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.highlight = this.highlight.next();
                        this.paint_conflicts(cx);
                        cx.notify();
                    })),
            )
            .child(
                Button::new("shape")
                    .small()
                    .ghost()
                    .label(format!(
                        "{}: {}",
                        tr("Connectors"),
                        tr(if curved { "Curved" } else { "Straight" })
                    ))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.surface.update(cx, |s, cx| {
                            s.curved = !s.curved;
                            cx.notify();
                        });
                        cx.notify();
                    })),
            )
    }

    fn comparison_menu(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let view = cx.entity().clone();
        settings::popover("comparison_menu")
            .anchor(Anchor::TopRight)
            .trigger(
                Button::new("comparison_menu-trigger")
                    .small()
                    .ghost()
                    .child(settings::icon("sliders-horizontal"))
                    .label(tr("Compare"))
                    .accessibility_label(tr("Compare"))
                    .tooltip(tr("Compare")),
            )
            .content(move |_, _, cx| {
                view.update(cx, |this, cx| this.comparison_controls(cx))
                    .into_any_element()
            })
    }

    fn action_controls(&self, cx: &mut Context<Self>) -> Div {
        let locked = self.selected.is_none() || self.saving;

        div()
            .w(px(290.))
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                Button::new("use-both")
                    .xsmall()
                    .ghost()
                    .label(tr("Use both"))
                    .disabled(locked)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.resolve(Some(Resolution::Both), window, cx)
                    })),
            )
            .child(
                Button::new("use-base")
                    .xsmall()
                    .ghost()
                    .label(tr("Use base"))
                    .disabled(locked)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.resolve(Some(Resolution::Base), window, cx)
                    })),
            )
            .child(
                Button::new("mark-resolved")
                    .xsmall()
                    .ghost()
                    .label(tr("Mark resolved"))
                    .disabled(locked)
                    .on_click(cx.listener(|this, _, window, cx| this.mark_resolved(window, cx))),
            )
            .child(
                Button::new("reset-conflict")
                    .xsmall()
                    .ghost()
                    .label(tr("Reset conflict"))
                    .disabled(locked)
                    .on_click(cx.listener(|this, _, window, cx| this.resolve(None, window, cx))),
            )
    }

    fn actions_menu(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let view = cx.entity().clone();
        settings::popover("actions_menu")
            .anchor(Anchor::TopRight)
            .trigger(
                Button::new("actions_menu-trigger")
                    .small()
                    .ghost()
                    .child(settings::icon("ellipsis"))
                    .accessibility_label(tr("Actions"))
                    .tooltip(tr("Actions")),
            )
            .content(move |_, _, cx| {
                view.update(cx, |this, cx| this.action_controls(cx))
                    .into_any_element()
            })
    }

    fn settings_menu(&self) -> impl IntoElement + use<> {
        let panel = self.settings.clone();
        settings::popover("settings-menu")
            .anchor(Anchor::TopRight)
            .trigger(
                Button::new("settings-trigger")
                    .small()
                    .ghost()
                    .child(settings::icon("settings"))
                    .accessibility_label(tr("Settings"))
                    .tooltip(tr("Settings")),
            )
            .content(move |_, _, _| panel.clone())
    }

    fn result_actions(&self, cx: &mut Context<Self>) -> Div {
        let locked = self.selected.is_none() || self.saving;
        div()
            .flex()
            .items_center()
            .gap_1()
            .pr_2()
            .child(
                Button::new("use-source")
                    .xsmall()
                    .ghost()
                    .child(surface::merge_chevrons(false))
                    .accessibility_label(tr("Source to Result · Alt+Right"))
                    .tooltip(tr("Source to Result · Alt+Right"))
                    .disabled(locked)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.resolve(Some(Resolution::Source), window, cx)
                    })),
            )
            .child(
                Button::new("use-target")
                    .xsmall()
                    .ghost()
                    .child(surface::merge_chevrons(true))
                    .accessibility_label(tr("Target to Result · Alt+Left"))
                    .tooltip(tr("Target to Result · Alt+Left"))
                    .disabled(locked)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.resolve(Some(Resolution::Target), window, cx)
                    })),
            )
            .child(
                Button::new("save")
                    .small()
                    .child(settings::icon("save"))
                    .accessibility_label(tr("Save"))
                    .tooltip(tr("Save"))
                    .disabled(!self.ready() || self.saving)
                    .on_click(cx.listener(|this, _, window, cx| this.save(false, window, cx))),
            )
            .child(
                Button::new("save-close")
                    .small()
                    .primary()
                    .child(settings::icon("check"))
                    .accessibility_label(tr("Save & Close"))
                    .tooltip(tr("Save & Close"))
                    .disabled(!self.ready() || self.saving)
                    .on_click(cx.listener(|this, _, window, cx| this.save(true, window, cx))),
            )
    }
}

impl Render for MergeApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_window_title(&tr("rdiff — Three-way merge"));
        let total = self.document.as_ref().map_or(0, |doc| doc.conflicts.len());
        let remaining = self.document.as_ref().map_or(0, MergeDocument::remaining);
        let locked = self.selected.is_none() || self.saving;
        self.surface.update(cx, |surface, _| {
            surface.disabled = self.saving || self.document.is_none()
        });
        let sync = self.surface.read(cx).sync_scroll;
        let collapsed = self.surface.read(cx).collapse_unchanged;
        let header = |label: &'static str, path: String, text: SharedString| {
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .items_center()
                .gap_2()
                .px_3()
                .text_xs()
                .child(tr(label))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(rgb(settings::color(MUTED)))
                        .child(path),
                )
                .child(surface::line_ending_badge(&text))
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(settings::color(BG)))
            .text_color(rgb(settings::color(TEXT)))
            .font_family("Segoe UI")
            .child(
                div()
                    .h(px(38.))
                    .flex_shrink_0()
                    .px_2()
                    .flex()
                    .items_center()
                    .gap_1()
                    .bg(rgb(settings::color(PANEL)))
                    .border_b_1()
                    .border_color(rgb(settings::color(BORDER)))
                    .child(
                        Button::new("previous")
                            .small()
                            .ghost()
                            .child(settings::icon("arrow-up"))
                            .accessibility_label(tr("Previous conflict · Alt+Up / Shift+F7"))
                            .tooltip(tr("Previous conflict · Alt+Up / Shift+F7"))
                            .disabled(locked)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.navigate(false, window, cx)),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(settings::color(MUTED)))
                            .child(format!(
                                "{} / {}",
                                self.selected.map_or(0, |index| index + 1),
                                total
                            )),
                    )
                    .child(
                        Button::new("next")
                            .small()
                            .ghost()
                            .child(settings::icon("arrow-down"))
                            .accessibility_label(tr("Next conflict · Alt+Down / F7"))
                            .tooltip(tr("Next conflict · Alt+Down / F7"))
                            .disabled(locked)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.navigate(true, window, cx)),
                            ),
                    )
                    .child(
                        Button::new("undo")
                            .small()
                            .ghost()
                            .child(settings::icon("undo-2"))
                            .accessibility_label(tr("Undo · Ctrl+Z"))
                            .tooltip(tr("Undo · Ctrl+Z"))
                            .disabled(
                                self.saving || self.document.as_ref().is_none_or(|d| !d.can_undo()),
                            )
                            .on_click(
                                cx.listener(|this, _, window, cx| this.history(false, window, cx)),
                            ),
                    )
                    .child(
                        Button::new("redo")
                            .small()
                            .ghost()
                            .child(settings::icon("redo-2"))
                            .accessibility_label(tr("Redo · Ctrl+Shift+Z"))
                            .tooltip(tr("Redo · Ctrl+Shift+Z"))
                            .disabled(
                                self.saving || self.document.as_ref().is_none_or(|d| !d.can_redo()),
                            )
                            .on_click(
                                cx.listener(|this, _, window, cx| this.history(true, window, cx)),
                            ),
                    )
                    .child(
                        div()
                            .w(px(1.))
                            .h(px(20.))
                            .mx_1()
                            .bg(rgb(settings::color(BORDER))),
                    )
                    .child(
                        Button::new("sync")
                            .small()
                            .ghost()
                            .w(px(30.))
                            .child(surface::toolbar_icon(false))
                            .accessibility_label(tr("Sync scroll"))
                            .tooltip(tr(if sync {
                                "Sync scroll: On"
                            } else {
                                "Sync scroll: Off"
                            }))
                            .selected(sync)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.surface.update(cx, |s, cx| {
                                    s.sync_scroll = !s.sync_scroll;
                                    cx.notify();
                                });
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("collapse-unchanged")
                            .small()
                            .ghost()
                            .w(px(30.))
                            .child(surface::toolbar_icon(true))
                            .accessibility_label(tr("Collapse unchanged"))
                            .selected(collapsed)
                            .disabled(locked)
                            .tooltip(tr(if collapsed {
                                "Collapse unchanged: On · Click to expand all"
                            } else {
                                "Collapse unchanged: Off · Click to collapse"
                            }))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.surface
                                    .update(cx, |surface, cx| surface.toggle_unchanged(cx));
                                cx.notify();
                            })),
                    )
                    .child(self.comparison_menu(cx))
                    .child(
                        Button::new("show-base")
                            .small()
                            .ghost()
                            .selected(self.show_base)
                            .label(tr("Base"))
                            .disabled(locked)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_base = !this.show_base;
                                cx.notify();
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(settings::color(if remaining == 0 {
                                GREEN
                            } else {
                                AMBER
                            })))
                            .child(if crate::i18n::korean() {
                                format!("미해결 {remaining} / {total}")
                            } else {
                                format!("{remaining} unresolved / {total}")
                            }),
                    )
                    .child(
                        Button::new("conflicts")
                            .small()
                            .ghost()
                            .label(tr("Conflicts"))
                            .selected(self.show_conflicts)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_conflicts = !this.show_conflicts;
                                cx.notify();
                            })),
                    )
                    .child(self.actions_menu(cx))
                    .child(self.settings_menu()),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .when(self.show_conflicts, |row| {
                        row.child(
                            div()
                                .w(px(155.))
                                .flex_shrink_0()
                                .p_2()
                                .border_r_1()
                                .border_color(rgb(settings::color(BORDER)))
                                .id("merge-conflicts")
                                .overflow_y_scroll()
                                .children(
                                    self.document
                                        .iter()
                                        .flat_map(|doc| doc.conflicts.iter().enumerate())
                                        .map(|(index, conflict)| {
                                            Button::new(("conflict", index))
                                                .tooltip(format!(
                                                    "{} {}",
                                                    tr("Base line"),
                                                    conflict.base_line
                                                ))
                                                .w_full()
                                                .mb_1()
                                                .small()
                                                .ghost()
                                                .selected(self.selected == Some(index))
                                                .label(format!(
                                                    "{} {}",
                                                    index + 1,
                                                    conflict.resolution.as_ref().map_or_else(
                                                        || tr("Unresolved"),
                                                        |r| tr(r.label())
                                                    )
                                                ))
                                                .disabled(self.saving)
                                                .on_click(cx.listener(
                                                    move |this, _, window, cx| {
                                                        this.select(index, window, cx)
                                                    },
                                                ))
                                        }),
                                ),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .child(
                                div()
                                    .h(px(34.))
                                    .flex_shrink_0()
                                    .flex()
                                    .border_b_1()
                                    .border_color(rgb(settings::color(BORDER)))
                                    .child(header(
                                        "Source",
                                        self.paths.source.display().to_string(),
                                        self.source.read(cx).value(),
                                    ))
                                    .child(div().w(px(48.)).flex_shrink_0())
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .flex()
                                            .items_center()
                                            .child(header(
                                                "Result",
                                                self.paths.result.display().to_string(),
                                                self.result.read(cx).value(),
                                            ))
                                            .child(self.result_actions(cx)),
                                    )
                                    .child(div().w(px(48.)).flex_shrink_0())
                                    .child(header(
                                        "Target",
                                        self.paths.target.display().to_string(),
                                        self.target.read(cx).value(),
                                    )),
                            )
                            .child(div().flex_1().min_h_0().child(self.surface.clone()))
                            .when(self.show_base, |view| {
                                view.child(
                                    div()
                                        .h(px(160.))
                                        .flex_shrink_0()
                                        .border_t_1()
                                        .border_color(rgb(settings::color(BORDER)))
                                        .child(self.pane(
                                            "BASE · selected conflict",
                                            self.paths.base.display().to_string(),
                                            &self.base,
                                            true,
                                            MUTED,
                                        )),
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .h(px(38.))
                    .flex_shrink_0()
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .bg(rgb(settings::color(PANEL)))
                    .border_t_1()
                    .border_color(rgb(settings::color(BORDER)))
                    .child(
                        div()
                            .flex_1()
                            .text_xs()
                            .text_color(rgb(settings::color(MUTED)))
                            .truncate()
                            .child(tr(&self.status)),
                    )
                    .child(
                        Button::new("close")
                            .small()
                            .ghost()
                            .label(tr("Close"))
                            .disabled(self.saving)
                            .on_click(cx.listener(|this, _, window, cx| {
                                if this.request_close(window, cx) {
                                    window.remove_window();
                                }
                            })),
                    ),
            )
    }
}
pub fn run(paths: MergePaths) -> u8 {
    let exit = Arc::new(AtomicU8::new(1));
    let app_exit = exit.clone();
    application()
        .with_assets(crate::assets::AppAssets)
        .run(move |cx| {
            init(cx);
            crate::syntax::init();
            settings::init(cx);
            let bounds = Bounds::centered(None, size(px(1480.), px(920.)), cx);
            let (_, view) = open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(1160.), px(720.))),
                    titlebar: Some(TitlebarOptions {
                        title: Some(tr("rdiff — Three-way merge").into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    let view = cx.new(|cx| MergeApp::new(paths, app_exit, window, cx));
                    let weak = view.downgrade();
                    window.on_window_should_close(cx, move |window, cx| {
                        weak.update(cx, |this, cx| this.request_close(window, cx))
                            .unwrap_or(true)
                    });
                    view
                },
            )
            .expect("Failed to open merge window");
            let view = view.downgrade();
            cx.intercept_keystrokes(move |event, window, cx| {
                let key = &event.keystroke;
                if settings::popup_open() {
                    return;
                }
                if key.modifiers.platform {
                    return;
                }
                if key.key == "escape"
                    && !key.modifiers.alt
                    && !key.modifiers.control
                    && !key.modifiers.shift
                {
                    let handled = view
                        .update(cx, |this, cx| {
                            if this.close_prompt {
                                return false;
                            }
                            if this.request_close(window, cx) {
                                window.remove_window();
                            }
                            true
                        })
                        .unwrap_or(false);
                    if handled {
                        cx.stop_propagation();
                    }
                    return;
                }
                let command = if key.modifiers.alt {
                    if key.modifiers.control || key.modifiers.shift {
                        return;
                    }
                    match key.key.as_str() {
                        "down" => 2,
                        "up" => 3,
                        "left" => 6,
                        "right" => 7,
                        _ => return,
                    }
                } else {
                    match (key.key.as_str(), key.modifiers.control, key.modifiers.shift) {
                        ("s", true, false) => 0,
                        ("enter", true, false) => 1,
                        ("f7", false, false) => 2,
                        ("f7", false, true) => 3,
                        ("z", true, false) => 4,
                        ("z", true, true) | ("y", true, false) => 5,
                        _ => return,
                    }
                };
                cx.stop_propagation();
                let _ = view.update(cx, |this, cx| {
                    if this.close_prompt {
                        return;
                    }
                    match command {
                        0 => this.save(false, window, cx),
                        1 => this.save(true, window, cx),
                        2 => this.navigate(true, window, cx),
                        3 => this.navigate(false, window, cx),
                        4 => this.history(false, window, cx),
                        5 => this.history(true, window, cx),
                        6 => this.resolve(Some(Resolution::Target), window, cx),
                        _ => this.resolve(Some(Resolution::Source), window, cx),
                    }
                });
            })
            .detach();
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            cx.activate(true);
        });
    exit.load(Ordering::SeqCst)
}
