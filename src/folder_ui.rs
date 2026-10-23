use crate::{
    diff,
    folder::{
        self, CompareOptions, CopyDirection, EntryKind, FolderComparison, SortColumn, Status,
    },
    i18n::tr,
    settings, surface, ui,
};
use gpui_kit::{
    component::{
        Disableable, Selectable, Sizable,
        button::{Button, ButtonVariants},
        input::{Input, InputEvent, InputState},
    },
    *,
};
use std::{
    ops::Range,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime},
};

const COLUMNS: [&str; 6] = [
    "File name",
    "Folder",
    "Comparison result",
    "Left modified",
    "Right modified",
    "Extension",
];
const SORT_COLUMNS: [SortColumn; 6] = [
    SortColumn::Name,
    SortColumn::Folder,
    SortColumn::Status,
    SortColumn::LeftModified,
    SortColumn::RightModified,
    SortColumn::Extension,
];

struct FolderApp {
    settings: Entity<settings::SettingsPanel>,
    paths: [Entity<InputState>; 2],
    filter: Entity<InputState>,
    recursive: bool,
    show_identical: bool,
    sort: SortColumn,
    descending: bool,
    comparison: Option<Arc<FolderComparison>>,
    compared_options: Option<CompareOptions>,
    visible: Vec<usize>,
    selected: Option<usize>,
    elapsed: Duration,
    busy: bool,
    picking: bool,
    opening: bool,
    copying: bool,
    copy_prompt: bool,
    copy_task: Option<Task<()>>,
    pending_selection: Option<PathBuf>,
    message: Option<String>,
    error: Option<String>,
    cancelled: Arc<AtomicBool>,
    scan_task: Option<Task<()>>,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
    _inputs: Vec<Subscription>,
}

impl Drop for FolderApp {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl FolderApp {
    fn new(paths: [Option<PathBuf>; 2], window: &mut Window, cx: &mut Context<Self>) -> Self {
        let preferences = cx.global::<settings::Preferences>().clone();
        let paths = std::array::from_fn(|index| {
            let path = paths[index]
                .as_ref()
                .or(preferences.folder_paths[index].as_ref())
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default();
            cx.new(|cx| {
                InputState::new(window, cx)
                    .placeholder(tr("Choose a folder or enter its path"))
                    .default_value(path)
            })
        });
        let filter = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("*.h;*.cpp;!*.tmp")
                .default_value(preferences.folder_filter)
        });
        let inputs = paths
            .iter()
            .chain(std::iter::once(&filter))
            .map(|input| {
                cx.subscribe(input, |this, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.compare(cx);
                    }
                    cx.notify();
                })
            })
            .collect();
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let mut this = Self {
            settings: cx
                .new(|cx| settings::SettingsPanel::new(settings::WindowKind::Folder, window, cx)),
            paths,
            filter,
            recursive: preferences.folder_recursive,
            show_identical: preferences.folder_show_identical,
            sort: preferences.folder_sort,
            descending: preferences.folder_sort_descending,
            comparison: None,
            compared_options: None,
            visible: Vec::new(),
            selected: None,
            elapsed: Duration::ZERO,
            busy: false,
            picking: false,
            opening: false,
            copying: false,
            copy_prompt: false,
            copy_task: None,
            pending_selection: None,
            message: None,
            error: None,
            cancelled: Arc::new(AtomicBool::new(false)),
            scan_task: None,
            scroll: UniformListScrollHandle::new(),
            focus,
            _inputs: inputs,
        };
        if this
            .roots(cx)
            .iter()
            .all(|path| !path.as_os_str().is_empty())
        {
            this.compare(cx);
        }
        this
    }

    fn roots(&self, cx: &App) -> [PathBuf; 2] {
        std::array::from_fn(|index| PathBuf::from(self.paths[index].read(cx).value().as_str()))
    }

    fn current(&self, cx: &App) -> bool {
        self.comparison
            .as_ref()
            .is_some_and(|comparison| comparison.roots == self.roots(cx))
            && self.compared_options.as_ref().is_some_and(|options| {
                options.recursive == self.recursive
                    && options.filter == self.filter.read(cx).value().as_str()
            })
    }

    fn remember(&mut self, change: impl Fn(&mut settings::Preferences), cx: &mut Context<Self>) {
        self.settings
            .update(cx, |panel, cx| panel.remember(change, cx));
    }

    fn compare(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.picking || self.copying || self.copy_prompt {
            return;
        }
        let roots = self.roots(cx);
        if roots.iter().any(|path| path.as_os_str().is_empty()) {
            self.error = Some(tr("Choose both folders before comparing"));
            cx.notify();
            return;
        }
        let options = CompareOptions {
            recursive: self.recursive,
            filter: self.filter.read(cx).value().to_string(),
        };
        self.remember(
            |preferences| {
                preferences.folder_paths = roots.clone().map(Some);
                preferences.folder_recursive = options.recursive;
                preferences.folder_filter = options.filter.clone();
            },
            cx,
        );
        self.cancelled.store(true, Ordering::Relaxed);
        self.cancelled = Arc::new(AtomicBool::new(false));
        let cancelled = self.cancelled.clone();
        let task_options = options.clone();
        self.busy = true;
        self.error = None;
        self.message = None;
        self.comparison = None;
        self.visible.clear();
        self.selected = None;
        let start = Instant::now();
        let scan = cx
            .background_executor()
            .spawn(async move { folder::compare(roots, task_options, &cancelled) });
        self.scan_task = Some(cx.spawn(async move |this, cx| {
            let result = scan.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                this.elapsed = start.elapsed();
                match result {
                    Ok(comparison) => {
                        this.comparison = Some(Arc::new(comparison));
                        this.compared_options = Some(options);
                        this.rebuild();
                        if let Some(relative) = this.pending_selection.take() {
                            this.selected = this
                                .visible
                                .iter()
                                .copied()
                                .find(|&index| {
                                    this.comparison.as_ref().unwrap().entries[index].relative
                                        == relative
                                })
                                .or(this.selected);
                        }
                    }
                    Err(error) => {
                        this.error = Some(format!("{}: {error}", tr("Folder comparison failed")))
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn cancel(&mut self, cx: &mut Context<Self>) {
        self.cancelled.store(true, Ordering::Relaxed);
        if self.copying {
            self.message = Some(tr("Canceling file copy…"));
            cx.notify();
            return;
        }
        self.scan_task = None;
        self.busy = false;
        self.error = Some(tr("Folder comparison canceled"));
        cx.notify();
    }

    fn rebuild(&mut self) {
        self.visible = self
            .comparison
            .as_ref()
            .map(|comparison| comparison.visible(self.show_identical, self.sort, self.descending))
            .unwrap_or_default();
        if !self
            .selected
            .is_some_and(|index| self.visible.contains(&index))
        {
            self.selected = self.visible.first().copied();
        }
        self.scroll = UniformListScrollHandle::new();
    }

    fn pick(&mut self, side: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.picking || self.copying || self.copy_prompt {
            return;
        }
        self.picking = true;
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(tr("Choose a folder").into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = picker.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.picking = false;
                match result {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.first() {
                            this.paths[side].update(cx, |input, cx| {
                                input.set_value(path.to_string_lossy().into_owned(), window, cx)
                            });
                            if this
                                .roots(cx)
                                .iter()
                                .all(|path| !path.as_os_str().is_empty())
                            {
                                this.compare(cx);
                            }
                        }
                    }
                    Ok(Ok(None)) => {}
                    error => {
                        this.error = Some(format!("{}: {error:?}", tr("Folder picker failed")))
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn open_selected(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.opening || self.copying || self.copy_prompt || !self.current(cx) {
            return;
        }
        let Some(comparison) = &self.comparison else {
            return;
        };
        let Some(entry) = self
            .selected
            .and_then(|index| comparison.entries.get(index))
            .filter(|entry| entry.can_open())
        else {
            return;
        };
        let paths = [
            entry
                .left
                .as_ref()
                .map(|_| comparison.roots[0].join(&entry.relative)),
            entry
                .right
                .as_ref()
                .map(|_| comparison.roots[1].join(&entry.relative)),
        ];
        let task_paths = paths.clone();
        self.opening = true;
        self.error = None;
        // Check text/size limits off the UI thread. Binary and oversized files
        // remain comparable in the table without opening an unusable editor.
        let read = cx.background_executor().spawn(async move {
            for path in task_paths.iter().flatten() {
                diff::read_text(path).map_err(|error| format!("{}: {error}", path.display()))?;
            }
            Ok::<_, String>(())
        });
        cx.spawn(async move |this, cx| {
            let result = read.await;
            let _ = this.update(cx, |this, cx| {
                this.opening = false;
                if let Err(error) = result.and_then(|_| ui::open_pair(paths, cx)) {
                    this.error = Some(error);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn can_copy(&self, direction: CopyDirection, cx: &App) -> bool {
        !self.busy
            && !self.picking
            && !self.opening
            && !self.copying
            && !self.copy_prompt
            && self.current(cx)
            && self
                .selected
                .and_then(|index| self.comparison.as_ref()?.entries.get(index))
                .is_some_and(|entry| entry.can_copy(direction))
    }

    fn request_copy(
        &mut self,
        direction: CopyDirection,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_copy(direction, cx) {
            return;
        }
        let comparison = self.comparison.as_ref().unwrap().clone();
        let index = self.selected.unwrap();
        let entry = &comparison.entries[index];
        if entry.info(1 - direction.source()).is_none() {
            self.start_copy(comparison, index, direction, cx);
            return;
        }
        let source = comparison.roots[direction.source()].join(&entry.relative);
        let target = comparison.roots[1 - direction.source()].join(&entry.relative);
        let sides = [tr("Left folder"), tr("Right folder")];
        let detail = format!(
            "{}: {}\n\n{} ({}):\n{}\n\n{} ({}):\n{}",
            tr("File name"),
            entry.relative.display(),
            tr("Copy from"),
            sides[direction.source()],
            source.display(),
            tr("Overwrite destination"),
            sides[1 - direction.source()],
            target.display()
        );
        let question = format!(
            "{}\n{}",
            tr(direction.label()),
            tr("Overwrite the destination file?")
        );
        let answers = [tr("Cancel"), tr("Overwrite")];
        let refs: Vec<_> = answers.iter().map(String::as_str).collect();
        self.copy_prompt = true;
        let prompt = window.prompt(PromptLevel::Warning, &question, Some(&detail), &refs, cx);
        cx.spawn_in(window, async move |this, cx| {
            let answer = prompt.await.ok();
            let _ = this.update(cx, |this, cx| {
                this.copy_prompt = false;
                if answer == Some(1) {
                    this.start_copy(comparison, index, direction, cx);
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn start_copy(
        &mut self,
        comparison: Arc<FolderComparison>,
        index: usize,
        direction: CopyDirection,
        cx: &mut Context<Self>,
    ) {
        if self.copying
            || !self.current(cx)
            || !self
                .comparison
                .as_ref()
                .is_some_and(|current| Arc::ptr_eq(current, &comparison))
        {
            return;
        }
        let relative = comparison.entries[index].relative.clone();
        self.cancelled.store(true, Ordering::Relaxed);
        self.cancelled = Arc::new(AtomicBool::new(false));
        let cancelled = self.cancelled.clone();
        self.copying = true;
        self.error = None;
        self.message = Some(format!("{} · {}", tr("Copying file…"), relative.display()));
        let copy = cx
            .background_executor()
            .spawn(async move { comparison.copy_file(index, direction, &cancelled) });
        self.copy_task = Some(cx.spawn(async move |this, cx| {
            let result = copy.await;
            let _ = this.update(cx, |this, cx| {
                this.copying = false;
                this.message = None;
                match result {
                    Ok(_) => {
                        this.pending_selection = Some(relative.clone());
                        this.compare(cx);
                        this.message = Some(format!(
                            "{} · {} · {}",
                            tr("Copied"),
                            tr(direction.label()),
                            relative.display()
                        ));
                    }
                    Err(error) => {
                        this.error = Some(format!("{}: {}", tr("Copy failed"), tr(error)))
                    }
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn path_row(&self, side: usize, cx: &mut Context<Self>) -> impl IntoElement {
        let label = if side == 0 {
            "Left folder"
        } else {
            "Right folder"
        };
        div()
            .flex()
            .items_center()
            .gap_2()
            .child(div().w(px(100.)).flex_shrink_0().text_sm().child(tr(label)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .drag_over::<ExternalPaths>(|style, _, _, _| {
                        style.bg(rgb(settings::color(0x32455b)))
                    })
                    .on_drop(cx.listener(move |this, files: &ExternalPaths, window, cx| {
                        if this.busy || this.picking || this.copying || this.copy_prompt {
                            return;
                        }
                        if let [path] = files.paths() {
                            this.paths[side].update(cx, |input, cx| {
                                input.set_value(path.to_string_lossy().into_owned(), window, cx)
                            });
                            this.compare(cx);
                        } else {
                            this.error = Some(tr("Drop one folder on each path field"));
                        }
                        cx.notify();
                    }))
                    .child(
                        Input::new(&self.paths[side])
                            .small()
                            .aria_label(tr(label))
                            .disabled(
                                self.busy || self.picking || self.copying || self.copy_prompt,
                            ),
                    ),
            )
            .child(
                Button::new(("browse-folder", side))
                    .small()
                    .ghost()
                    .child(settings::icon("folder-open"))
                    .label(tr("Browse…"))
                    .accessibility_label(format!("{}: {}", tr("Browse…"), tr(label)))
                    .disabled(self.busy || self.picking || self.copying || self.copy_prompt)
                    .on_click(cx.listener(move |this, _, window, cx| this.pick(side, window, cx))),
            )
    }

    fn table_header(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .h(px(30.))
            .flex_shrink_0()
            .bg(rgb(settings::color(surface::PANEL)))
            .border_b_1()
            .border_color(rgb(settings::color(surface::BORDER)))
            .children(COLUMNS.iter().enumerate().map(|(column, label)| {
                let sorted = self.sort == SORT_COLUMNS[column];
                let label = format!(
                    "{}{}",
                    tr(label),
                    if sorted {
                        if self.descending { " ▼" } else { " ▲" }
                    } else {
                        ""
                    }
                );
                cell(
                    column,
                    Button::new(("folder-sort", column))
                        .small()
                        .ghost()
                        .w_full()
                        .label(label)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let sort = SORT_COLUMNS[column];
                            this.descending = this.sort == sort && !this.descending;
                            this.sort = sort;
                            let descending = this.descending;
                            this.remember(
                                |preferences| {
                                    preferences.folder_sort = sort;
                                    preferences.folder_sort_descending = descending;
                                },
                                cx,
                            );
                            this.rebuild();
                            cx.notify();
                        })),
                )
            }))
    }

    fn row(&self, row: usize, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let index = self.visible[row];
        let entry = &self.comparison.as_ref().unwrap().entries[index];
        let (color, background) = match entry.status {
            Status::Different => (0xe2c88a, 0x3b3423),
            Status::LeftOnly | Status::RightOnly => (0x8491a3, 0x242629),
            Status::TypeMismatch | Status::Error => (0xf0a1a8, 0x2d2027),
            Status::Identical => (0x83d6ad, surface::BACKGROUND),
            Status::NotCompared => (0x8491a3, surface::BACKGROUND),
        };
        let kind = if entry
            .left
            .iter()
            .chain(entry.right.iter())
            .any(|info| info.kind == EntryKind::Directory)
        {
            "[+] "
        } else if entry
            .left
            .iter()
            .chain(entry.right.iter())
            .any(|info| info.kind == EntryKind::Link)
        {
            "[↗] "
        } else {
            ""
        };
        div()
            .id(("folder-entry", index))
            .role(Role::Row)
            .aria_label(format!(
                "{} · {} · {} · {}",
                entry.relative.display(),
                tr(entry.status.label()),
                date(entry.left.as_ref().and_then(|info| info.modified)),
                date(entry.right.as_ref().and_then(|info| info.modified))
            ))
            .aria_selected(self.selected == Some(index))
            .aria_description(entry.error.clone().unwrap_or_default())
            .flex()
            .items_center()
            .h(px(28.))
            .w_full()
            .text_xs()
            .bg(rgb(settings::color(if self.selected == Some(index) {
                0x32455b
            } else {
                background
            })))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if this.copying || this.copy_prompt {
                        return;
                    }
                    this.selected = Some(index);
                    window.focus(&this.focus, cx);
                    if event.click_count == 2 {
                        this.open_selected(cx);
                    }
                    cx.notify();
                }),
            )
            .child(cell(
                0,
                div().truncate().child(format!("{kind}{}", entry.name())),
            ))
            .child(cell(
                1,
                div()
                    .truncate()
                    .text_color(rgb(settings::color(0x8491a3)))
                    .child(entry.folder()),
            ))
            .child(cell(
                2,
                div()
                    .truncate()
                    .text_color(rgb(settings::color(color)))
                    .child(tr(entry.status.label())),
            ))
            .child(cell(
                3,
                date(entry.left.as_ref().and_then(|info| info.modified)),
            ))
            .child(cell(
                4,
                date(entry.right.as_ref().and_then(|info| info.modified)),
            ))
            .child(cell(5, div().truncate().child(entry.extension())))
    }

    fn settings_menu(&self) -> impl IntoElement + use<> {
        let panel = self.settings.clone();
        settings::popover("settings-menu")
            .anchor(Anchor::TopRight)
            .trigger(
                Button::new("folder-settings")
                    .small()
                    .ghost()
                    .child(settings::icon("settings"))
                    .accessibility_label(tr("Settings"))
                    .tooltip(tr("Settings")),
            )
            .content(move |_, _, _| panel.clone())
    }
}

fn cell(column: usize, child: impl IntoElement) -> Div {
    let cell = div()
        .min_w_0()
        .px_2()
        .h_full()
        .flex()
        .items_center()
        .overflow_hidden()
        .child(child);
    match column {
        0 | 1 => cell.flex_1(),
        2 => cell.w(px(176.)).flex_shrink_0(),
        3 | 4 => cell.w(px(148.)).flex_shrink_0(),
        _ => cell.w(px(96.)).flex_shrink_0(),
    }
}

fn date(time: Option<SystemTime>) -> String {
    time.map(|time| {
        chrono::DateTime::<chrono::Local>::from(time)
            .format("%Y-%m-%d %H:%M")
            .to_string()
    })
    .unwrap_or_else(|| "—".into())
}

impl Render for FolderApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_window_title(&tr("rdiff — Folder comparison"));
        let current = self.current(cx);
        let can_open = current
            && self
                .selected
                .and_then(|index| self.comparison.as_ref()?.entries.get(index))
                .is_some_and(|entry| entry.can_open());
        let summary = if let Some(comparison) = &self.comparison {
            let count = |status| {
                comparison
                    .entries
                    .iter()
                    .filter(|entry| entry.status == status)
                    .count()
            };
            format!(
                "{} / {} {} · {} {} · {} {} · {} {} · {} {} · {:.2} s",
                self.visible.len(),
                comparison.entries.len(),
                tr("items"),
                count(Status::Different),
                tr("Different"),
                count(Status::LeftOnly),
                tr("Left only"),
                count(Status::RightOnly),
                tr("Right only"),
                count(Status::Error),
                tr("Errors"),
                self.elapsed.as_secs_f64()
            )
        } else if self.busy {
            tr("Comparing folder contents…")
        } else {
            tr("Choose two folders and press Compare")
        };
        let detail = self
            .error
            .clone()
            .or_else(|| self.message.clone())
            .unwrap_or_else(|| {
                if !current && self.comparison.is_some() {
                    return tr("Paths or options changed. Compare again.");
                }
                self.selected
                    .and_then(|index| self.comparison.as_ref()?.entries.get(index))
                    .map(|entry| {
                        if let Some(error) = &entry.error {
                            format!("{}: {error}", entry.relative.display())
                        } else {
                            format!(
                                "{} · {}",
                                entry.relative.display(),
                                tr("Double-click a file or press Enter to open its diff")
                            )
                        }
                    })
                    .unwrap_or_else(|| tr("Double-click a file or press Enter to open its diff"))
            });
        let body = if self.visible.is_empty() {
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .text_color(rgb(settings::color(0x8491a3)))
                .child(tr(if self.busy {
                    "Comparing folder contents…"
                } else if self.comparison.is_some() {
                    "No items match the current view"
                } else {
                    "Choose two folders and press Compare"
                }))
                .into_any_element()
        } else {
            uniform_list(
                "folder-results",
                self.visible.len(),
                cx.processor(|this, range: Range<usize>, _, cx| {
                    range.map(|row| this.row(row, cx)).collect()
                }),
            )
            .track_scroll(&self.scroll)
            .flex_1()
            .min_h_0()
            .into_any_element()
        };
        div()
            .key_context("RdiffFolders")
            .track_focus(&self.focus)
            .flex()
            .flex_col()
            .size_full()
            .bg(rgb(settings::color(surface::BACKGROUND)))
            .text_color(rgb(settings::color(0xd6dfeb)))
            .font_family("Segoe UI")
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if settings::popup_open() || this.copy_prompt {
                    return;
                }
                match event.keystroke.key.as_str() {
                    "f5" => {
                        this.compare(cx);
                        cx.stop_propagation();
                    }
                    "escape" => {
                        if this.busy || this.copying {
                            this.cancel(cx);
                        } else {
                            window.remove_window();
                        }
                        cx.stop_propagation();
                    }
                    "enter" if this.focus.is_focused(window) => {
                        this.open_selected(cx);
                        cx.stop_propagation();
                    }
                    "right" | "left"
                        if event.keystroke.modifiers.alt
                            && !event.keystroke.modifiers.control
                            && !event.keystroke.modifiers.shift
                            && !event.keystroke.modifiers.platform
                            && this.focus.is_focused(window) =>
                    {
                        this.request_copy(
                            if event.keystroke.key == "right" {
                                CopyDirection::LeftToRight
                            } else {
                                CopyDirection::RightToLeft
                            },
                            window,
                            cx,
                        );
                        cx.stop_propagation();
                    }
                    "down" | "up" if this.focus.is_focused(window) && !this.visible.is_empty() => {
                        let index = this
                            .selected
                            .and_then(|selected| {
                                this.visible.iter().position(|&index| index == selected)
                            })
                            .unwrap_or(0);
                        let row = if event.keystroke.key == "down" {
                            (index + 1).min(this.visible.len() - 1)
                        } else {
                            index.saturating_sub(1)
                        };
                        this.selected = Some(this.visible[row]);
                        this.scroll.scroll_to_item(row, ScrollStrategy::Center);
                        cx.notify();
                        cx.stop_propagation();
                    }
                    _ => {}
                }
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_shrink_0()
                    .p_3()
                    .gap_2()
                    .bg(rgb(settings::color(surface::PANEL)))
                    .child(self.path_row(0, cx))
                    .child(self.path_row(1, cx))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("folder-swap")
                                    .small()
                                    .ghost()
                                    .label(tr("Swap folders"))
                                    .disabled(
                                        self.busy
                                            || self.picking
                                            || self.copying
                                            || self.copy_prompt,
                                    )
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        let left = this.paths[0].read(cx).value();
                                        let right = this.paths[1].read(cx).value();
                                        this.paths[0].update(cx, |input, cx| {
                                            input.set_value(right, window, cx)
                                        });
                                        this.paths[1].update(cx, |input, cx| {
                                            input.set_value(left, window, cx)
                                        });
                                        this.compare(cx);
                                    })),
                            )
                            .child(
                                Button::new("folder-recursive")
                                    .small()
                                    .ghost()
                                    .label(tr("Include subfolders"))
                                    .selected(self.recursive)
                                    .disabled(self.busy || self.copying || self.copy_prompt)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.recursive = !this.recursive;
                                        let recursive = this.recursive;
                                        this.remember(
                                            |preferences| preferences.folder_recursive = recursive,
                                            cx,
                                        );
                                        cx.notify();
                                    })),
                            )
                            .child(div().text_xs().child(tr("File filter")))
                            .child(
                                Input::new(&self.filter)
                                    .small()
                                    .w(px(180.))
                                    .aria_label(tr("File filter"))
                                    .disabled(self.busy || self.copying || self.copy_prompt),
                            )
                            .child(div().flex_1())
                            .child(
                                Button::new("folder-compare")
                                    .small()
                                    .label(tr(if self.busy || self.copying {
                                        "Cancel"
                                    } else {
                                        "Compare folders"
                                    }))
                                    .disabled(self.picking || self.copy_prompt)
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if this.busy || this.copying {
                                            this.cancel(cx);
                                        } else {
                                            this.compare(cx);
                                        }
                                    })),
                            )
                            .child(self.settings_menu()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .h(px(36.))
                    .flex_shrink_0()
                    .px_2()
                    .gap_2()
                    .border_y_1()
                    .border_color(rgb(settings::color(surface::BORDER)))
                    .child(
                        Button::new("folder-show-identical")
                            .small()
                            .ghost()
                            .label(tr("Show identical"))
                            .selected(self.show_identical)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_identical = !this.show_identical;
                                let show = this.show_identical;
                                this.remember(
                                    |preferences| preferences.folder_show_identical = show,
                                    cx,
                                );
                                this.rebuild();
                                cx.notify();
                            })),
                    )
                    .child(
                        div()
                            .flex_1()
                            .text_xs()
                            .text_color(rgb(settings::color(0x8491a3)))
                            .truncate()
                            .child(summary),
                    )
                    .child(
                        Button::new("folder-copy-to-right")
                            .small()
                            .ghost()
                            .label(tr(CopyDirection::LeftToRight.label()))
                            .tooltip(tr("Copy the selected file to the right folder · Alt+Right"))
                            .disabled(!self.can_copy(CopyDirection::LeftToRight, cx))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.request_copy(CopyDirection::LeftToRight, window, cx)
                            })),
                    )
                    .child(
                        Button::new("folder-copy-to-left")
                            .small()
                            .ghost()
                            .label(tr(CopyDirection::RightToLeft.label()))
                            .tooltip(tr("Copy the selected file to the left folder · Alt+Left"))
                            .disabled(!self.can_copy(CopyDirection::RightToLeft, cx))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.request_copy(CopyDirection::RightToLeft, window, cx)
                            })),
                    )
                    .child(
                        Button::new("folder-open-file")
                            .small()
                            .ghost()
                            .label(tr("Open file diff"))
                            .disabled(
                                !can_open
                                    || self.opening
                                    || self.busy
                                    || self.copying
                                    || self.copy_prompt,
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.open_selected(cx))),
                    ),
            )
            .child(
                div()
                    .id("folder-table-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_x_scroll()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .w_full()
                            .h_full()
                            .min_w(px(1040.))
                            .child(self.table_header(cx))
                            .child(body),
                    ),
            )
            .child(
                div()
                    .h(px(30.))
                    .flex_shrink_0()
                    .px_3()
                    .flex()
                    .items_center()
                    .text_xs()
                    .border_t_1()
                    .border_color(rgb(settings::color(surface::BORDER)))
                    .text_color(rgb(settings::color(if self.error.is_some() {
                        0xf0a1a8
                    } else {
                        0x8491a3
                    })))
                    .overflow_hidden()
                    .child(detail),
            )
    }
}

pub fn open(paths: [Option<PathBuf>; 2], cx: &mut App) -> Result<(), String> {
    let kind = settings::WindowKind::Folder;
    open_window(
        WindowOptions {
            window_bounds: Some(kind.bounds(cx)),
            window_min_size: Some(kind.min_size()),
            titlebar: Some(TitlebarOptions {
                title: Some(tr("rdiff — Folder comparison").into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        cx,
        |window, cx| {
            let view = cx.new(|cx| FolderApp::new(paths, window, cx));
            let weak = view.downgrade();
            window.on_window_should_close(cx, move |_, cx| {
                weak.update(cx, |this, _| !this.copying && !this.copy_prompt)
                    .unwrap_or(true)
            });
            view
        },
    )
    .map_err(|error| error.to_string())?;
    cx.activate(true);
    Ok(())
}

pub fn run(paths: Vec<PathBuf>) {
    application()
        .with_assets(crate::assets::AppAssets)
        .run(move |cx| {
            init(cx);
            crate::syntax::init();
            settings::init(cx);
            open([paths.first().cloned(), paths.get(1).cloned()], cx)
                .expect("Failed to open folder comparison window");
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            cx.activate(true);
        });
}
