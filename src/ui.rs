use crate::diff::{self, Comparison, Line};
use crate::surface::{self, Change, DiffSurface, SurfaceEvent};
use crate::{i18n::tr, settings};
use gpui_kit::component::{
    Disableable, Selectable, Sizable,
    button::{Button, ButtonVariants},
    input::{EditorState, InputEvent},
};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use std::{ops::Range, path::PathBuf, sync::Arc};

const BG: u32 = surface::BACKGROUND;
const PANEL: u32 = surface::PANEL;
const BORDER: u32 = surface::BORDER;
const MUTED: u32 = 0x8491a3;
const TEXT: u32 = 0xd6dfeb;
const GREEN: u32 = 0x83d6ad;
const RED: u32 = 0xf0a1a8;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Both,
    Left,
    Right,
}

#[derive(Clone, Copy)]
struct DisplayRow {
    source: usize,
    side: Side,
}

struct BufferSnapshot {
    left: String,
    right: String,
}

struct LoadedFile {
    side: Side,
    path: PathBuf,
    text: String,
    bom: bool,
}

fn read_files(files: Vec<(Side, PathBuf)>) -> Result<Vec<LoadedFile>, String> {
    files
        .into_iter()
        .map(|(side, path)| {
            let (text, bom) = diff::read_text(&path)
                .map_err(|error| format!("Could not open {}: {error}", path.display()))?;
            Ok(LoadedFile {
                side,
                path,
                text,
                bom,
            })
        })
        .collect()
}

struct DiffApp {
    settings: Entity<settings::SettingsPanel>,
    surface: Entity<DiffSurface>,
    _surface_subscription: Subscription,
    left: Entity<EditorState>,
    right: Entity<EditorState>,
    left_name: String,
    right_name: String,
    left_path: Option<PathBuf>,
    right_path: Option<PathBuf>,
    left_saved: String,
    right_saved: String,
    left_bom: bool,
    right_bom: bool,
    left_dirty: bool,
    right_dirty: bool,
    undo: Vec<BufferSnapshot>,
    redo: Vec<BufferSnapshot>,
    right_current: String,
    saving: bool,
    close_prompt: bool,
    pending_selection: Option<usize>,
    horizontal: [f32; 2],
    _right_subscription: Subscription,
    comparison: Arc<Comparison>,
    display_rows: Arc<Vec<DisplayRow>>,
    change_rows: Vec<usize>,
    selected: Option<usize>,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
    unified: bool,
    whitespace: diff::Whitespace,
    highlight: diff::HighlightMode,
    show_changes: bool,
    busy: bool,
    revision: usize,
    status: String,
    comparison_task: Option<Task<()>>,
}

impl DiffApp {
    fn new(paths: &[PathBuf], window: &mut Window, cx: &mut Context<Self>) -> Self {
        let preferences = cx.global::<settings::Preferences>().clone();
        let mut left_text = String::new();
        let mut right_text = String::new();
        let mut left_name = "No file selected".to_owned();
        let mut right_name = "No file selected".to_owned();
        let mut status = "Open files or drop them here to start a comparison".to_owned();
        let mut errors = Vec::new();
        let (mut left_path, mut right_path) = (None, None);
        let (mut left_bom, mut right_bom) = (false, false);
        if !paths.is_empty() {
            left_name = paths[0].display().to_string();
            right_name = paths[1].display().to_string();
        }
        for (index, path) in paths.iter().enumerate() {
            match diff::read_text(path) {
                Ok((text, bom)) => {
                    if index == 0 {
                        left_text = text;
                        left_name = path.display().to_string();
                        left_path = Some(path.clone());
                        left_bom = bom;
                    } else {
                        right_text = text;
                        right_name = path.display().to_string();
                        right_path = Some(path.clone());
                        right_bom = bom;
                    }
                    status = "Files loaded · UTF-8".into();
                }
                Err(error) => errors.push(format!("Could not open {}: {error}", path.display())),
            }
        }
        if !errors.is_empty() {
            status = errors.join(" · ");
        }
        let left = cx.new(|cx| {
            EditorState::new(window, cx)
                .language(surface::language(&left_name))
                .default_value(left_text.clone())
        });
        let right = cx.new(|cx| {
            EditorState::new(window, cx)
                .language(surface::language(&right_name))
                .default_value(right_text.clone())
        });
        let surface = cx.new(|cx| {
            DiffSurface::new(
                vec![left.clone(), right.clone()],
                vec![true, false],
                window,
                cx,
            )
        });
        let surface_subscription = cx.subscribe_in(
            &surface,
            window,
            |this, _, event: &SurfaceEvent, window, cx| match *event {
                SurfaceEvent::Apply { index, .. } => this.apply_right(index, window, cx),
                SurfaceEvent::Select(index) => this.select_change(index, cx),
                SurfaceEvent::DropFiles { pane, ref paths } => this.drop_files(
                    if pane == 0 { Side::Left } else { Side::Right },
                    paths,
                    window,
                    cx,
                ),
            },
        );
        let right_subscription = cx.subscribe(&right, |this, state, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                let text = state.read(cx).value().to_string();
                if text == this.right_current {
                    return;
                }
                this.remember_buffers(cx);
                this.right_current = text;
                this.right_dirty = this.right_current != this.right_saved;
                this.compare(cx);
            }
        });
        let comparison = Arc::new(diff::compare_with_whitespace(
            &left_text,
            &right_text,
            preferences.whitespace,
        ));
        let focus = cx.focus_handle();
        window.focus(&focus, cx);
        let mut this = Self {
            settings: cx
                .new(|cx| settings::SettingsPanel::new(settings::WindowKind::Diff, window, cx)),
            surface,
            _surface_subscription: surface_subscription,
            left,
            right,
            left_name,
            right_name,
            left_path,
            right_path,
            left_saved: left_text,
            right_saved: right_text.clone(),
            right_current: right_text,
            left_bom,
            right_bom,
            left_dirty: false,
            right_dirty: false,
            undo: Vec::new(),
            redo: Vec::new(),
            saving: false,
            close_prompt: false,
            pending_selection: None,
            horizontal: [0., 0.],
            _right_subscription: right_subscription,
            comparison,
            display_rows: Arc::default(),
            change_rows: Vec::new(),
            selected: None,
            scroll: UniformListScrollHandle::new(),
            focus,
            unified: preferences.unified,
            whitespace: preferences.whitespace,
            highlight: preferences.highlight,
            show_changes: preferences.show_changes,
            busy: false,
            revision: 0,
            status,
            comparison_task: None,
        };
        this.rebuild_rows();
        this.update_surface(cx);
        this
    }

    fn remember(&mut self, change: impl Fn(&mut settings::Preferences), cx: &mut Context<Self>) {
        self.settings
            .update(cx, |panel, cx| panel.remember(change, cx));
    }

    fn update_surface(&mut self, cx: &mut Context<Self>) {
        let changes = self
            .comparison
            .edits
            .iter()
            .enumerate()
            .map(|(id, edit)| Change {
                id,
                ranges: vec![edit.left.clone(), edit.right.clone()],
                color: if edit.left.is_empty() {
                    surface::INSERTED
                } else if edit.right.is_empty() {
                    surface::DELETED
                } else {
                    surface::MODIFIED
                },
                resolved: false,
            })
            .collect();
        let inline = self
            .comparison
            .inline_ranges(self.highlight)
            .into_iter()
            .collect();
        self.surface.update(cx, |surface, cx| {
            surface.highlighting = self.highlight != diff::HighlightMode::None;
            surface.set_changes(changes, self.selected, inline, cx);
        });
    }

    fn rebuild_rows(&mut self) {
        let mut rows = Vec::new();
        let mut changes = Vec::new();
        let mut index = 0;
        while index < self.comparison.rows.len() {
            let row = &self.comparison.rows[index];
            if self.comparison.hunks.binary_search(&index).is_ok() {
                changes.push(rows.len());
            }
            if self.unified && row.changed {
                let start = index;
                while index < self.comparison.rows.len() && self.comparison.rows[index].changed {
                    index += 1;
                }
                for side in [Side::Left, Side::Right] {
                    for source in start..index {
                        let row = &self.comparison.rows[source];
                        let line = if side == Side::Left {
                            &row.left
                        } else {
                            &row.right
                        };
                        if line.is_some() {
                            rows.push(DisplayRow { source, side });
                        }
                    }
                }
            } else {
                rows.push(DisplayRow {
                    source: index,
                    side: Side::Both,
                });
                index += 1;
            }
        }
        self.display_rows = Arc::new(rows);
        self.change_rows = changes;
        self.selected = None;
        self.scroll = UniformListScrollHandle::new();
    }

    fn compare(&mut self, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        let left = self.left.read(cx).value().to_string();
        let right = self.right.read(cx).value().to_string();
        if left.len() > diff::MAX_BYTES || right.len() > diff::MAX_BYTES {
            self.status = "Text is too large. The limit is 2 MiB per side.".into();
            cx.notify();
            return;
        }
        self.busy = true;
        self.revision += 1;
        let revision = self.revision;
        let whitespace = self.whitespace;
        let task = cx.background_executor().spawn(async move {
            Arc::new(diff::compare_with_whitespace(&left, &right, whitespace))
        });
        self.comparison_task = Some(cx.spawn(async move |this, cx| {
            let comparison = task.await;
            let _ = this.update(cx, |this, cx| {
                if this.revision != revision {
                    return;
                }
                this.comparison = comparison;
                this.busy = false;
                this.rebuild_rows();
                this.update_surface(cx);
                if let Some(index) = this
                    .pending_selection
                    .take()
                    .filter(|_| !this.change_rows.is_empty())
                {
                    this.select_change(index.min(this.change_rows.len() - 1), cx);
                }
                this.status = if this.left_dirty || this.right_dirty {
                    "Unsaved changes · Ctrl+S to save · Undo / Redo: Ctrl+Z / Ctrl+Shift+Z"
                } else if this.comparison.hunks.is_empty() {
                    if this.whitespace != diff::Whitespace::Exact {
                        "No differences with whitespace ignored"
                    } else {
                        "No differences (CRLF and LF are treated equally)"
                    }
                } else {
                    "Comparison ready · F7 next change · Shift+F7 previous change"
                }
                .into();
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn open_file(&mut self, side: Side, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.can_replace_files(side, cx) {
            return;
        }
        self.busy = true;
        cx.notify();
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Choose a UTF-8 text file".into()),
        });
        cx.spawn_in(window, async move |this, cx| match picker.await {
            Ok(Ok(Some(paths))) => {
                let Some(path) = paths.into_iter().next() else {
                    let _ = this.update(cx, |this, cx| {
                        this.busy = false;
                        cx.notify();
                    });
                    return;
                };
                let _ = this.update_in(cx, |this, window, cx| {
                    this.busy = false;
                    this.load_files(vec![(side, path)], window, cx);
                });
            }
            Ok(Ok(None)) => {
                let _ = this.update(cx, |this, cx| {
                    this.busy = false;
                    cx.notify();
                });
            }
            error => {
                let _ = this.update(cx, |this, cx| {
                    this.busy = false;
                    this.status = format!("File picker failed: {error:?}");
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn can_replace_files(&mut self, side: Side, cx: &mut Context<Self>) -> bool {
        if self.saving || self.close_prompt {
            return false;
        }
        if (side != Side::Right && self.left_dirty) || (side != Side::Left && self.right_dirty) {
            self.status = "Save or undo changes before replacing a modified file.".into();
            cx.notify();
            return false;
        }
        true
    }

    fn drop_files(
        &mut self,
        side: Side,
        paths: &[PathBuf],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let files = match paths {
            [path] => vec![(side, path.clone())],
            [left, right] => vec![(Side::Left, left.clone()), (Side::Right, right.clone())],
            _ => {
                self.status =
                    "Drop one file onto either side, or two files to compare them.".into();
                cx.notify();
                return;
            }
        };
        self.load_files(files, window, cx);
    }

    fn load_files(
        &mut self,
        files: Vec<(Side, PathBuf)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let side = if files.len() == 2 {
            Side::Both
        } else {
            files[0].0
        };
        if self.busy || !self.can_replace_files(side, cx) {
            return;
        }
        self.busy = true;
        cx.notify();
        // Read every destination before changing either buffer, so a bad second
        // file never leaves half of a newly dropped comparison open.
        let task = cx
            .background_executor()
            .spawn(async move { read_files(files) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                // Edits can arrive while the files are being read.
                if !this.can_replace_files(side, cx) {
                    cx.notify();
                    return;
                }
                match result {
                    Ok(files) => {
                        this.undo.clear();
                        this.redo.clear();
                        for LoadedFile {
                            side,
                            path,
                            text,
                            bom,
                        } in files
                        {
                            let name = path.display().to_string();
                            let editor = if side == Side::Left {
                                this.left_path = Some(path);
                                this.left_saved = text.clone();
                                this.left_bom = bom;
                                this.left_dirty = false;
                                this.left_name = name.clone();
                                &this.left
                            } else {
                                this.right_path = Some(path);
                                this.right_saved = text.clone();
                                this.right_current = text.clone();
                                this.right_bom = bom;
                                this.right_dirty = false;
                                this.right_name = name.clone();
                                &this.right
                            };
                            editor.update(cx, |state, cx| {
                                state.set_value(text, window, cx);
                                state.set_highlighter(surface::language(&name), cx);
                            });
                        }
                        window.focus(&this.focus, cx);
                        this.compare(cx);
                    }
                    Err(error) => {
                        this.status = error;
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn navigate(&mut self, forward: bool, cx: &mut Context<Self>) {
        let count = self.change_rows.len();
        if count == 0 || self.busy || self.saving {
            return;
        }
        let index = match self.selected {
            None => {
                if forward {
                    0
                } else {
                    count - 1
                }
            }
            Some(index) => {
                if forward {
                    (index + 1) % count
                } else {
                    (index + count - 1) % count
                }
            }
        };
        self.select_change(index, cx);
    }

    fn select_change(&mut self, index: usize, cx: &mut Context<Self>) {
        if self.busy || self.saving {
            return;
        }
        if let Some(&row) = self.change_rows.get(index) {
            self.selected = Some(index);
            self.scroll.scroll_to_item(row, ScrollStrategy::Center);
            self.surface
                .update(cx, |surface, cx| surface.select(index, false, None, cx));
            cx.notify();
        }
    }

    fn swap(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.saving || self.right_dirty || self.left_dirty {
            return;
        }
        let left = self.left.read(cx).value();
        let right = self.right.read(cx).value();
        self.left
            .update(cx, |state, cx| state.set_value(right, window, cx));
        self.right
            .update(cx, |state, cx| state.set_value(left, window, cx));
        std::mem::swap(&mut self.left_name, &mut self.right_name);
        std::mem::swap(&mut self.left_path, &mut self.right_path);
        std::mem::swap(&mut self.left_saved, &mut self.right_saved);
        std::mem::swap(&mut self.left_bom, &mut self.right_bom);
        self.right_dirty = self.right.read(cx).value().as_ref() != self.right_saved;
        self.right_current = self.right.read(cx).value().to_string();
        self.left.update(cx, |state, cx| {
            state.set_highlighter(surface::language(&self.left_name), cx)
        });
        self.right.update(cx, |state, cx| {
            state.set_highlighter(surface::language(&self.right_name), cx)
        });
        self.undo.clear();
        self.redo.clear();
        self.horizontal = [0., 0.];
        window.focus(&self.focus, cx);
        self.compare(cx);
    }

    fn snapshot(&self, cx: &Context<Self>) -> BufferSnapshot {
        BufferSnapshot {
            left: self.left.read(cx).value().to_string(),
            right: self.right_current.clone(),
        }
    }

    fn remember_buffers(&mut self, cx: &Context<Self>) {
        self.redo.clear();
        self.undo.push(self.snapshot(cx));
        while self.undo.len() > 128
            || self
                .undo
                .iter()
                .map(|s| s.left.len() + s.right.len())
                .sum::<usize>()
                > 32 * 1024 * 1024
        {
            self.undo.remove(0);
        }
    }

    fn apply_right(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.saving {
            return;
        }
        let left = self.left.read(cx).value().to_string();
        let right = self.right.read(cx).value().to_string();
        match self.comparison.apply_right(index, &left, &right) {
            Ok(text) => {
                self.remember_buffers(cx);
                self.right_dirty = text != self.right_saved;
                self.right_current = text.clone();
                self.right
                    .update(cx, |state, cx| state.set_value(text, window, cx));
                self.pending_selection = Some(index);
                self.compare(cx);
            }
            Err(error) => {
                self.status = error;
                cx.notify();
            }
        }
    }

    fn apply_left(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.saving {
            return;
        }
        let left = self.left.read(cx).value().to_string();
        let right = self.right.read(cx).value().to_string();
        match self.comparison.apply_left(index, &left, &right) {
            Ok(text) => {
                self.remember_buffers(cx);
                self.left_dirty = text != self.left_saved;
                self.left
                    .update(cx, |state, cx| state.set_value(text, window, cx));
                self.pending_selection = Some(index);
                self.compare(cx);
            }
            Err(error) => {
                self.status = error;
                cx.notify();
            }
        }
    }

    fn apply_selected(&mut self, to_left: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.selected {
            if to_left {
                self.apply_left(index, window, cx);
            } else {
                self.apply_right(index, window, cx);
            }
        }
    }

    fn undo_right(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.saving {
            return;
        }
        if let Some(snapshot) = self.undo.pop() {
            self.redo.push(self.snapshot(cx));
            self.restore_buffers(snapshot, window, cx);
            self.compare(cx);
        }
    }

    fn redo_right(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.saving {
            return;
        }
        if let Some(snapshot) = self.redo.pop() {
            self.undo.push(self.snapshot(cx));
            self.restore_buffers(snapshot, window, cx);
            self.compare(cx);
        }
    }

    fn restore_buffers(
        &mut self,
        snapshot: BufferSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.left_dirty = snapshot.left != self.left_saved;
        self.right_dirty = snapshot.right != self.right_saved;
        self.right_current = snapshot.right.clone();
        for (editor, text) in [(&self.left, snapshot.left), (&self.right, snapshot.right)] {
            let selection = editor.read(cx).selected_range();
            let start = text.floor_char_boundary(selection.start.min(text.len()));
            let end = text.floor_char_boundary(selection.end.min(text.len()));
            editor.update(cx, |state, cx| {
                state.set_value(text, window, cx);
                state.set_selected_range(start..end, cx);
            });
        }
        self.right.update(cx, |state, cx| state.focus(window, cx));
        self.pending_selection = None;
    }

    fn revert_right(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.saving || !self.right_dirty {
            return;
        }
        self.remember_buffers(cx);
        self.right_current = self.right_saved.clone();
        self.right.update(cx, |state, cx| {
            state.set_value(self.right_saved.clone(), window, cx)
        });
        self.right_dirty = false;
        self.compare(cx);
    }

    fn save_files(&mut self, side: Side, close: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.saving {
            return;
        }
        let mut jobs = Vec::new();
        for (candidate, dirty, path, expected, text, bom) in [
            (
                Side::Left,
                self.left_dirty,
                &self.left_path,
                &self.left_saved,
                self.left.read(cx).value(),
                self.left_bom,
            ),
            (
                Side::Right,
                self.right_dirty,
                &self.right_path,
                &self.right_saved,
                self.right.read(cx).value(),
                self.right_bom,
            ),
        ] {
            if !dirty || (side != Side::Both && side != candidate) {
                continue;
            }
            let Some(path) = path.clone() else {
                self.status =
                    "Open a file on the modified side to enable saving. Changes stay in memory."
                        .into();
                cx.notify();
                return;
            };
            jobs.push((candidate, path, expected.clone(), text.to_string(), bom));
        }
        if jobs.is_empty() {
            if close && !self.left_dirty && !self.right_dirty {
                window.remove_window();
            }
            return;
        }
        self.saving = true;
        self.status = "Saving modified files…".into();
        // Each file is replaced atomically. Keep successful saves and failures
        // separate so a failed second save never clears its dirty state.
        let task = cx.background_executor().spawn(async move {
            jobs.into_iter()
                .map(|(side, path, expected, text, bom)| {
                    let result = diff::save_file(&path, &expected, &text, bom);
                    (side, path, text, result)
                })
                .collect::<Vec<_>>()
        });
        cx.spawn_in(window, async move |this, cx| {
            let results = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.saving = false;
                let mut messages = Vec::new();
                for (side, path, saved, result) in results {
                    match result {
                        Ok(()) => {
                            if side == Side::Left {
                                this.left_saved = saved;
                                this.left_dirty =
                                    this.left.read(cx).value().as_ref() != this.left_saved;
                            } else {
                                this.right_saved = saved;
                                this.right_dirty =
                                    this.right.read(cx).value().as_ref() != this.right_saved;
                            }
                            messages.push(format!("Saved {}", path.display()));
                        }
                        Err(error) => {
                            messages.push(format!("Save failed for {}: {error}", path.display()))
                        }
                    }
                }
                this.status = messages.join(" · ");
                if close && !this.left_dirty && !this.right_dirty {
                    window.remove_window();
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
        if !self.right_dirty && !self.left_dirty {
            return true;
        }
        self.close_prompt = true;
        let can_save = (!self.left_dirty || self.left_path.is_some())
            && (!self.right_dirty || self.right_path.is_some());
        let answers = if can_save {
            vec!["Save", "Discard", "Cancel"]
        } else {
            vec!["Discard", "Cancel"]
        };
        let answers: Vec<String> = answers.into_iter().map(tr).collect();
        let answer_refs: Vec<&str> = answers.iter().map(String::as_str).collect();
        let prompt = window.prompt(
            PromptLevel::Warning,
            &tr("Save changes to the modified files?"),
            Some(&tr("Changes on either side will be lost if discarded.")),
            &answer_refs,
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let answer = prompt.await.ok();
            let _ = this.update_in(cx, |this, window, cx| {
                this.close_prompt = false;
                if can_save && answer == Some(0) {
                    this.save_files(Side::Both, true, window, cx);
                } else if answer == Some(usize::from(can_save)) {
                    window.remove_window();
                }
                cx.notify();
            });
        })
        .detach();
        false
    }

    fn copy_patch(&mut self, cx: &mut Context<Self>) {
        let left = self.left.read(cx).value().to_string();
        let right = self.right.read(cx).value().to_string();
        if left.len().max(right.len()) > diff::MAX_BYTES {
            return;
        }
        let patch = diff::unified_patch(&left, &right, &self.left_name, &self.right_name);
        if patch.is_empty() {
            self.status = "The files are identical; there is no patch to copy.".into();
        } else {
            cx.write_to_clipboard(ClipboardItem::new_string(patch));
            self.status = "Unified patch copied · Uses exact source text, including whitespace and line endings".into();
        }
        cx.notify();
    }

    fn file_header(&self, side: Side, cx: &mut Context<Self>) -> impl IntoElement {
        let text = if side == Side::Left {
            self.left.read(cx).value()
        } else {
            self.right.read(cx).value()
        };
        let (title, name, color, dirty, path) = if side == Side::Left {
            (
                "ORIGINAL",
                &self.left_name,
                RED,
                self.left_dirty,
                &self.left_path,
            )
        } else {
            (
                "MODIFIED",
                &self.right_name,
                GREEN,
                self.right_dirty,
                &self.right_path,
            )
        };
        let name = if path.is_none() {
            tr(name)
        } else {
            name.clone()
        };
        let name = if dirty { format!("{name}  •") } else { name };
        let disabled = self.busy || self.saving;
        div()
            .flex()
            .flex_1()
            .min_w_0()
            .items_center()
            .gap_2()
            .px_3()
            .h(px(34.))
            .drag_over::<ExternalPaths>(|style, _, _, _| style.bg(rgb(settings::color(0x32455b))))
            .on_drop(cx.listener(move |this, files: &ExternalPaths, window, cx| {
                this.drop_files(side, files.paths(), window, cx)
            }))
            .child(
                div()
                    .text_xs()
                    .text_color(rgb(settings::color(color)))
                    .child(tr(title)),
            )
            .child(div().flex_1().min_w_0().text_sm().truncate().child(name))
            .child(surface::line_ending_badge(&text))
            .child(
                Button::new(if side == Side::Left {
                    "open-left"
                } else {
                    "open-right"
                })
                .small()
                .ghost()
                .child(settings::icon("folder-open"))
                .accessibility_label(tr("Open file"))
                .tooltip(tr("Open file"))
                .disabled(disabled || dirty)
                .on_click(cx.listener(move |this, _, window, cx| this.open_file(side, window, cx))),
            )
            .child(
                Button::new(if side == Side::Left {
                    "save-left"
                } else {
                    "save-right"
                })
                .small()
                .ghost()
                .child(settings::icon("save"))
                .accessibility_label(tr(if side == Side::Left {
                    "Save left"
                } else {
                    "Save right"
                }))
                .tooltip(tr(if side == Side::Left {
                    "Save left"
                } else {
                    "Save right"
                }))
                .disabled(disabled || !dirty || path.is_none())
                .on_click(
                    cx.listener(move |this, _, window, cx| {
                        this.save_files(side, false, window, cx)
                    }),
                ),
            )
    }

    fn sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let hunks = self.comparison.hunks.clone();
        div().flex().flex_col().w(px(208.)).flex_shrink_0().bg(rgb(settings::color(PANEL)))
            .border_r_1().border_color(rgb(settings::color(BORDER))).p_4().gap_3()
            .child(div().text_xs().text_color(rgb(settings::color(MUTED))).child(tr("COMPARISON")))
            .child(div().text_lg().font_weight(FontWeight::SEMIBOLD).child(tr("Changes")))
            .child(div().flex().gap_3().text_sm()
                .child(div().text_color(rgb(settings::color(GREEN))).child(tr(format!("+{} additions", self.comparison.added))))
                .child(div().text_color(rgb(settings::color(RED))).child(format!("−{}", self.comparison.removed))))
            .child(div().h(px(1.)).bg(rgb(settings::color(BORDER))))
            .child(div().text_xs().text_color(rgb(settings::color(MUTED))).child(tr(format!("{} CHANGE GROUPS", hunks.len()))))
            .child(div().id("change-list").flex_1().min_h_0().overflow_y_scroll()
                .children(hunks.iter().enumerate().map(|(index, &source)| {
                    let row = &self.comparison.rows[source];
                    let line = row.right.as_ref().or(row.left.as_ref()).map_or(1, |line| line.number);
                    Button::new(("change", index)).w_full().mb_2().small().ghost()
                        .selected(self.selected == Some(index)).disabled(self.busy || self.saving)
                        .label(format!("{:02}    {} {line}", index + 1, tr("Line")))
                        .on_click(cx.listener(move |this, _, _, cx| this.select_change(index, cx)))
                })))
            .child(div().text_xs().text_color(rgb(settings::color(MUTED))).line_height(relative(1.7))
                .child(tr("F7  Next change\nShift+F7  Previous\nCtrl+E  Focus right\nCtrl+Enter  Compare")))
            .child(div().text_xs().text_color(rgb(settings::color(GREEN))).child(tr("●  LOCAL FILES ONLY")))
    }

    fn comparison_view(&self, window: &Window, cx: &Context<Self>) -> AnyElement {
        if self.comparison.rows.is_empty() {
            return div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(settings::color(MUTED)))
                .child(tr("No text · Open files or press Ctrl+E to enter text"))
                .into_any_element();
        }
        let comparison = self.comparison.clone();
        let tab_size = cx.global::<settings::Preferences>().editor_tab_size();
        let rows = self.display_rows.clone();
        let unified = self.unified;
        let selected_source = self.selected.map(|index| self.comparison.hunks[index]);
        let viewport_width = (f32::from(window.viewport_size().width)
            - if self.show_changes { 210. } else { 2. })
        .max(500.);
        let column_width = if unified {
            viewport_width
        } else {
            (viewport_width - 44.) / 2.
        };
        let content_width = self
            .comparison
            .rows
            .iter()
            .flat_map(|row| [&row.left, &row.right])
            .filter_map(|line| line.as_ref())
            .map(|line| line.text.len() + line.text.matches('\t').count() * (tab_size.tab_size - 1))
            .max()
            .unwrap_or(0) as f32
            * 8.
            + 200.;
        let max_scroll = (content_width - (column_width - 75.)).max(0.);
        let horizontal = self.horizontal.map(|offset| offset.min(max_scroll));
        let disabled = self.busy || self.saving;
        let view = cx.entity().downgrade();
        let highlight = self.highlight;
        uniform_list("diff-lines", rows.len(), move |range, _, _| {
            range
                .map(|index| {
                    let display = rows[index];
                    let row = &comparison.rows[display.source];
                    let active = selected_source.is_some_and(|start| {
                        let end = comparison
                            .hunks
                            .iter()
                            .copied()
                            .find(|&next| next > start)
                            .unwrap_or(comparison.rows.len());
                        row.changed && display.source >= start && display.source < end
                    });
                    let cell = |line, spans: &[Range<usize>], sign, color, side: usize| {
                        let view = view.clone();
                        code_cell(
                            line,
                            spans,
                            CellStyle {
                                changed: row.changed && highlight != diff::HighlightMode::None,
                                sign,
                                color,
                                active,
                                width: column_width,
                                content_width,
                                offset: horizontal[side],
                                tab_size,
                            },
                        )
                        .on_scroll_wheel(move |event, _, cx| {
                            let delta = event.delta.pixel_delta(px(26.));
                            let amount = if event.modifiers.shift && delta.x == px(0.) {
                                -f32::from(delta.y)
                            } else {
                                -f32::from(delta.x)
                            };
                            if amount.abs() > 0.1 {
                                let _ = view.update(cx, |this, cx| {
                                    this.horizontal[side] =
                                        (horizontal[side] + amount).clamp(0., max_scroll);
                                    cx.notify();
                                });
                                cx.stop_propagation();
                            }
                        })
                    };
                    let mut element = div()
                        .flex()
                        .h(px(26.))
                        .min_h(px(26.))
                        .w(px(viewport_width))
                        .font_family("Cascadia Mono")
                        .text_size(px(13.))
                        .line_height(px(26.));
                    if unified {
                        let inline = match (highlight, &row.left, &row.right) {
                            (diff::HighlightMode::Words, Some(left), Some(right))
                                if row.changed =>
                            {
                                diff::inline_changes(&left.text, &right.text, highlight)
                            }
                            (diff::HighlightMode::Characters, _, _) => {
                                [row.left_spans.clone(), row.right_spans.clone()]
                            }
                            _ => [Vec::new(), Vec::new()],
                        };
                        let (line, spans, sign, color) = match display.side {
                            Side::Left => (row.left.as_ref(), inline[0].as_slice(), "−", RED),
                            Side::Right => (row.right.as_ref(), inline[1].as_slice(), "+", GREEN),
                            Side::Both => {
                                (row.right.as_ref().or(row.left.as_ref()), &[][..], " ", TEXT)
                            }
                        };
                        element = element.child(cell(line, spans, sign, color, 0));
                    } else {
                        let mut gutter = div()
                            .w(px(44.))
                            .h_full()
                            .flex_shrink_0()
                            .flex()
                            .items_center()
                            .justify_center()
                            .bg(rgb(settings::color(if active {
                                0x2b4058
                            } else if row.changed {
                                0x1d2a3a
                            } else {
                                PANEL
                            })))
                            .border_r_1()
                            .border_color(rgb(settings::color(BORDER)));
                        if let Ok(hunk) = comparison.hunks.binary_search(&display.source) {
                            let view = view.clone();
                            gutter = gutter.child(
                                Button::new(("apply-right", hunk))
                                    .xsmall()
                                    .ghost()
                                    .w(px(38.))
                                    .h(px(24.))
                                    .label(tr(">>"))
                                    .disabled(disabled)
                                    .accessibility_label(tr("Apply this change from left to right"))
                                    .tooltip(tr("Apply this change from left to right"))
                                    .on_click(move |_, window, cx| {
                                        let _ = view.update(cx, |this, cx| {
                                            this.apply_right(hunk, window, cx)
                                        });
                                    }),
                            );
                        }
                        element = element
                            .child(cell(row.left.as_ref(), &row.left_spans, "−", RED, 0))
                            .child(gutter)
                            .child(cell(row.right.as_ref(), &row.right_spans, "+", GREEN, 1));
                    }
                    element
                })
                .collect::<Vec<_>>()
        })
        .track_scroll(&self.scroll)
        .size_full()
        .into_any_element()
    }
}

struct CellStyle {
    changed: bool,
    sign: &'static str,
    color: u32,
    active: bool,
    width: f32,
    content_width: f32,
    offset: f32,
    tab_size: gpui_kit::component::input::TabSize,
}

fn code_cell(line: Option<&Line>, spans: &[Range<usize>], style: CellStyle) -> Div {
    let CellStyle {
        changed,
        sign,
        color,
        active,
        width,
        content_width,
        offset,
        tab_size,
    } = style;
    let has_change = changed && line.is_some();
    let background = if has_change {
        if color == RED {
            if active { 0x42272f } else { 0x2d2027 }
        } else if active {
            0x1c3c31
        } else {
            0x192d26
        }
    } else if changed {
        0x131820
    } else {
        BG
    };
    let mut text = String::new();
    let mut highlights = Vec::new();
    if let Some(line) = line {
        let (expanded, positions) = tab_size.expand_tabs(&line.text);
        text = expanded;
        highlights = spans
            .iter()
            .filter(|range| !range.is_empty())
            .map(|range| {
                (
                    positions[range.start]..positions[range.end],
                    HighlightStyle {
                        background_color: Some(
                            rgb(settings::color(if color == RED {
                                0x75404e
                            } else {
                                0x32674f
                            }))
                            .into(),
                        ),
                        color: Some(rgb(settings::color(0xffffff)).into()),
                        ..Default::default()
                    },
                )
            })
            .collect();
        if !line.terminated {
            text.push_str("  ⏎ no newline");
        }
    }
    div()
        .flex()
        .w(px(width))
        .flex_shrink_0()
        .h_full()
        .bg(rgb(settings::color(background)))
        .border_r_1()
        .border_color(rgb(settings::color(BORDER)))
        .child(
            div()
                .w(px(3.))
                .flex_shrink_0()
                .h_full()
                .bg(rgb(settings::color(if active && has_change {
                    color
                } else {
                    background
                }))),
        )
        .child(
            div()
                .w(px(50.))
                .flex_shrink_0()
                .pr_2()
                .text_right()
                .text_color(rgb(settings::color(MUTED)))
                .child(line.map(|line| line.number.to_string()).unwrap_or_default()),
        )
        .child(
            div()
                .w(px(22.))
                .flex_shrink_0()
                .text_color(rgb(settings::color(color)))
                .child(if has_change { sign } else { " " }),
        )
        .child(
            div().flex_1().min_w_0().h_full().overflow_hidden().child(
                div()
                    .relative()
                    .left(px(-offset))
                    .w(px(content_width))
                    .text_color(rgb(settings::color(if has_change { color } else { TEXT })))
                    .whitespace_nowrap()
                    .child(StyledText::new(text).with_highlights(highlights)),
            ),
        )
}

impl DiffApp {
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
                        let highlight = this.highlight;
                        this.remember(|preferences| preferences.highlight = highlight, cx);
                        this.update_surface(cx);
                        cx.notify();
                    })),
            )
            .child(
                Button::new("whitespace")
                    .small()
                    .ghost()
                    .label(format!(
                        "{}: {}",
                        tr("Whitespace"),
                        tr(self.whitespace.label())
                    ))
                    .accessibility_label(tr("Whitespace comparison policy"))
                    .tooltip(tr("Whitespace comparison policy"))
                    .disabled(self.saving)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.whitespace = this.whitespace.next();
                        let whitespace = this.whitespace;
                        this.remember(|preferences| preferences.whitespace = whitespace, cx);
                        this.compare(cx);
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
                        let curved = this.surface.update(cx, |s, cx| {
                            s.curved = !s.curved;
                            cx.notify();
                            s.curved
                        });
                        this.remember(|preferences| preferences.curved = curved, cx);
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
        let disabled = self.busy || self.saving;

        div()
            .w(px(290.))
            .p_2()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                Button::new("apply-selected-left")
                    .xsmall()
                    .ghost()
                    .label(tr("<< Apply"))
                    .accessibility_label(tr("Apply selected change to the left · Alt+Left"))
                    .tooltip(tr("Apply selected change to the left · Alt+Left"))
                    .disabled(disabled || self.selected.is_none())
                    .on_click(
                        cx.listener(|this, _, window, cx| this.apply_selected(true, window, cx)),
                    ),
            )
            .child(
                Button::new("apply-selected")
                    .xsmall()
                    .ghost()
                    .label(tr("Apply >>"))
                    .accessibility_label(tr("Apply selected change to the right · Alt+Right"))
                    .tooltip(tr("Apply selected change to the right · Alt+Right"))
                    .disabled(disabled || self.selected.is_none())
                    .on_click(cx.listener(|this, _, window, cx| {
                        if let Some(i) = this.selected {
                            this.apply_right(i, window, cx);
                        }
                    })),
            )
            .child(
                Button::new("swap")
                    .xsmall()
                    .ghost()
                    .label(tr("Swap"))
                    .disabled(disabled || self.right_dirty || self.left_dirty)
                    .on_click(cx.listener(|this, _, window, cx| this.swap(window, cx))),
            )
            .child(
                Button::new("copy")
                    .xsmall()
                    .ghost()
                    .label(tr("Copy patch"))
                    .disabled(disabled)
                    .on_click(cx.listener(|this, _, _, cx| this.copy_patch(cx))),
            )
            .child(
                Button::new("revert-right")
                    .xsmall()
                    .ghost()
                    .label(tr("Revert right"))
                    .disabled(disabled || !self.right_dirty)
                    .on_click(cx.listener(|this, _, window, cx| this.revert_right(window, cx))),
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
}

impl Render for DiffApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        window.set_window_title(&tr("rdiff — Text comparison"));
        let disabled = self.busy || self.saving;
        self.surface
            .update(cx, |surface, _| surface.disabled = self.saving);
        let body = if self.unified {
            self.comparison_view(window, cx)
        } else {
            self.surface.clone().into_any_element()
        };
        let sync = self.surface.read(cx).sync_scroll;
        let collapsed = self.surface.read(cx).collapse_unchanged;
        div()
            .key_context("Rdiff")
            .track_focus(&self.focus)
            .flex()
            .flex_col()
            .size_full()
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
                            .accessibility_label(tr("Previous difference · Alt+Up / Shift+F7"))
                            .tooltip(tr("Previous difference · Alt+Up / Shift+F7"))
                            .disabled(disabled)
                            .on_click(cx.listener(|this, _, _, cx| this.navigate(false, cx))),
                    )
                    .child(
                        div()
                            .min_w(px(48.))
                            .text_center()
                            .text_xs()
                            .text_color(rgb(settings::color(MUTED)))
                            .child(format!(
                                "{} / {}",
                                self.selected.map_or(0, |index| index + 1),
                                self.comparison.hunks.len()
                            )),
                    )
                    .child(
                        Button::new("next")
                            .small()
                            .ghost()
                            .child(settings::icon("arrow-down"))
                            .accessibility_label(tr("Next difference · Alt+Down / F7"))
                            .tooltip(tr("Next difference · Alt+Down / F7"))
                            .disabled(disabled)
                            .on_click(cx.listener(|this, _, _, cx| this.navigate(true, cx))),
                    )
                    .child(
                        Button::new("undo-right")
                            .small()
                            .ghost()
                            .child(settings::icon("undo-2"))
                            .accessibility_label(tr("Undo · Ctrl+Z"))
                            .tooltip(tr("Undo · Ctrl+Z"))
                            .disabled(disabled || self.undo.is_empty())
                            .on_click(
                                cx.listener(|this, _, window, cx| this.undo_right(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("redo-right")
                            .small()
                            .ghost()
                            .child(settings::icon("redo-2"))
                            .accessibility_label(tr("Redo · Ctrl+Shift+Z"))
                            .tooltip(tr("Redo · Ctrl+Shift+Z"))
                            .disabled(disabled || self.redo.is_empty())
                            .on_click(
                                cx.listener(|this, _, window, cx| this.redo_right(window, cx)),
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
                                let sync = this.surface.update(cx, |s, cx| {
                                    s.sync_scroll = !s.sync_scroll;
                                    cx.notify();
                                    s.sync_scroll
                                });
                                this.remember(|preferences| preferences.sync_scroll = sync, cx);
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
                            .selected(collapsed && !self.unified)
                            .disabled(self.unified || self.busy || self.saving)
                            .tooltip(tr(if collapsed {
                                "Collapse unchanged: On · Click to expand all"
                            } else {
                                "Collapse unchanged: Off · Click to collapse"
                            }))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.surface
                                    .update(cx, |surface, cx| surface.toggle_unchanged(cx));
                                let collapsed = this.surface.read(cx).collapse_unchanged;
                                this.remember(
                                    |preferences| preferences.collapse_unchanged = collapsed,
                                    cx,
                                );
                                cx.notify();
                            })),
                    )
                    .child(self.comparison_menu(cx))
                    .child(div().flex_1())
                    .child(
                        Button::new("split")
                            .small()
                            .ghost()
                            .label(tr("Side by side"))
                            .selected(!self.unified)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.unified = false;
                                this.remember(|preferences| preferences.unified = false, cx);
                                this.rebuild_rows();
                                this.update_surface(cx);
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("unified")
                            .small()
                            .ghost()
                            .label(tr("Unified"))
                            .selected(self.unified)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.unified = true;
                                this.remember(|preferences| preferences.unified = true, cx);
                                this.rebuild_rows();
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("changes")
                            .small()
                            .ghost()
                            .child(settings::icon("list"))
                            .accessibility_label(tr("Changes"))
                            .tooltip(tr("Changes"))
                            .selected(self.show_changes)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_changes = !this.show_changes;
                                let show = this.show_changes;
                                this.remember(|preferences| preferences.show_changes = show, cx);
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
                    .when(self.show_changes, |row| row.child(self.sidebar(cx)))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .on_drop(cx.listener(|this, files: &ExternalPaths, window, cx| {
                                this.drop_files(Side::Right, files.paths(), window, cx);
                            }))
                            .child(
                                div()
                                    .flex()
                                    .h(px(34.))
                                    .flex_shrink_0()
                                    .border_b_1()
                                    .border_color(rgb(settings::color(BORDER)))
                                    .child(self.file_header(Side::Left, cx))
                                    .child(
                                        div()
                                            .w(self.surface.read(cx).gutter_width(cx))
                                            .flex_shrink_0(),
                                    )
                                    .child(self.file_header(Side::Right, cx)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_h_0()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .child(body),
                            ),
                    ),
            )
            .child(
                div()
                    .h(px(26.))
                    .flex_shrink_0()
                    .px_3()
                    .flex()
                    .items_center()
                    .gap_3()
                    .bg(rgb(settings::color(PANEL)))
                    .border_t_1()
                    .border_color(rgb(settings::color(BORDER)))
                    .text_xs()
                    .child(
                        div()
                            .text_color(rgb(settings::color(
                                if self.right_dirty || self.left_dirty {
                                    0xd9bc7c
                                } else {
                                    MUTED
                                },
                            )))
                            .child(tr(if self.saving {
                                "Saving…"
                            } else if self.busy {
                                "Comparing…"
                            } else if self.right_dirty || self.left_dirty {
                                "Modified"
                            } else {
                                "Ready"
                            })),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_color(rgb(settings::color(MUTED)))
                            .child(tr(&self.status)),
                    )
                    .child("UTF-8")
                    .child(div().text_xs().child(format!(
                        "Alt+↑↓ · {}    Alt+←→ · {}    Esc · {}",
                        tr("Navigate"),
                        tr("Apply"),
                        tr("Close")
                    ))),
            )
    }
}
pub fn run(paths: Vec<PathBuf>) {
    application()
        .with_assets(crate::assets::AppAssets)
        .run(move |cx| {
            init(cx);
            crate::syntax::init();
            settings::init(cx);
            let kind = settings::WindowKind::Diff;
            let (_, view) = open_window(
                WindowOptions {
                    window_bounds: Some(kind.bounds(cx)),
                    window_min_size: Some(kind.min_size()),
                    titlebar: Some(TitlebarOptions {
                        title: Some(tr("rdiff — Text comparison").into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                cx,
                |window, cx| {
                    let view = cx.new(|cx| DiffApp::new(&paths, window, cx));
                    let weak = view.downgrade();
                    window.on_window_should_close(cx, move |window, cx| {
                        weak.update(cx, |this, cx| this.request_close(window, cx))
                            .unwrap_or(true)
                    });
                    view
                },
            )
            .expect("Failed to open rdiff window");
            // Editor key bindings handle Ctrl+Enter themselves, before element
            // key-down callbacks. Intercept application commands before dispatch
            // so Compare cannot accidentally insert a newline into either buffer.
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
                        "left" => 7,
                        "right" => 8,
                        _ => return,
                    }
                } else {
                    match (key.key.as_str(), key.modifiers.control, key.modifiers.shift) {
                        ("enter", true, false) => 0,
                        ("e", true, false) => 1,
                        ("f7", false, false) => 2,
                        ("f7", false, true) => 3,
                        ("z", true, false) => 4,
                        ("s", true, false) => 5,
                        ("z", true, true) | ("y", true, false) => 6,
                        _ => return,
                    }
                };
                cx.stop_propagation();
                let _ = view.update(cx, |this, cx| {
                    if this.close_prompt {
                        return;
                    }
                    match command {
                        0 => {
                            window.focus(&this.focus, cx);
                            this.compare(cx);
                        }
                        1 => {
                            this.unified = false;
                            this.right.update(cx, |state, cx| state.focus(window, cx));
                            cx.notify();
                        }
                        2 => this.navigate(true, cx),
                        3 => this.navigate(false, cx),
                        4 => this.undo_right(window, cx),
                        5 => this.save_files(Side::Both, false, window, cx),
                        6 => this.redo_right(window, cx),
                        7 => this.apply_selected(true, window, cx),
                        _ => this.apply_selected(false, window, cx),
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
}

#[cfg(test)]
mod tests {
    use super::{Side, read_files};
    use std::fs;

    #[test]
    fn opens_a_pair_with_unicode_paths_and_preserves_bom_and_line_endings() {
        let directory = tempfile::tempdir().unwrap();
        let left = directory.path().join("원본 파일.rs");
        let right = directory.path().join("수정 파일.txt");
        fs::write(&left, "\u{feff}원본\r\n").unwrap();
        fs::write(&right, "수정\n마지막 줄").unwrap();

        let files = read_files(vec![
            (Side::Left, left.clone()),
            (Side::Right, right.clone()),
        ])
        .ok()
        .unwrap();
        assert_eq!(files.len(), 2);
        assert!(matches!(files[0].side, Side::Left));
        assert_eq!(files[0].path, left);
        assert_eq!(files[0].text, "원본\r\n");
        assert!(files[0].bom);
        assert!(matches!(files[1].side, Side::Right));
        assert_eq!(files[1].path, right);
        assert_eq!(files[1].text, "수정\n마지막 줄");
        assert!(!files[1].bom);
    }

    #[test]
    fn rejects_the_whole_pair_when_either_file_cannot_be_opened() {
        let directory = tempfile::tempdir().unwrap();
        let valid = directory.path().join("valid.txt");
        let binary = directory.path().join("binary.dat");
        let oversized = directory.path().join("oversized.txt");
        fs::write(&valid, "Current text\n").unwrap();
        fs::write(&binary, [0xff, 0xfe, 0]).unwrap();
        fs::write(&oversized, vec![b'a'; crate::diff::MAX_BYTES + 1]).unwrap();

        for invalid in [
            directory.path().join("missing.txt"),
            directory.path().to_path_buf(),
            binary,
            oversized,
        ] {
            for files in [
                vec![(Side::Left, valid.clone()), (Side::Right, invalid.clone())],
                vec![(Side::Left, invalid.clone()), (Side::Right, valid.clone())],
            ] {
                let error = read_files(files).err().expect("Must reject the whole pair");
                assert!(error.contains(&invalid.display().to_string()), "{error}");
            }
        }
        assert_eq!(fs::read_to_string(valid).unwrap(), "Current text\n");
    }
}
