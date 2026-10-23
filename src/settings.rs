use crate::{diff, i18n::tr};
use gpui_kit::{
    component::{
        Icon, Selectable, Sizable,
        button::{Button, ButtonVariants},
        input::TabSize,
        popover::Popover,
        theme::{Theme, ThemeMode},
    },
    *,
};
use serde::{Deserialize, Serialize};
use std::{
    io::Write,
    path::PathBuf,
    sync::atomic::{AtomicBool, AtomicU8, Ordering},
    time::Duration,
};

static DARK: AtomicBool = AtomicBool::new(false);
static POPUPS: AtomicU8 = AtomicU8::new(0);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    System,
    En,
    Ko,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    Light,
    Dark,
    #[default]
    System,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub language: Language,
    pub theme: Appearance,
    #[serde(deserialize_with = "deserialize_tab_size")]
    pub tab_size: usize,
    pub unified: bool,
    pub whitespace: diff::Whitespace,
    pub highlight: diff::HighlightMode,
    pub sync_scroll: bool,
    pub curved: bool,
    pub collapse_unchanged: bool,
    pub show_changes: bool,
    pub show_conflicts: bool,
    pub show_base: bool,
    pub diff_window: Option<SavedWindow>,
    pub merge_window: Option<SavedWindow>,
    pub folder_window: Option<SavedWindow>,
    pub folder_paths: [Option<PathBuf>; 2],
    pub folder_recursive: bool,
    pub folder_filter: String,
    pub folder_show_identical: bool,
    pub folder_sort: crate::folder::SortColumn,
    pub folder_sort_descending: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            language: Language::default(),
            theme: Appearance::default(),
            tab_size: 4,
            unified: false,
            whitespace: diff::Whitespace::Exact,
            highlight: diff::HighlightMode::Words,
            sync_scroll: true,
            curved: true,
            collapse_unchanged: true,
            show_changes: false,
            show_conflicts: false,
            show_base: false,
            diff_window: None,
            merge_window: None,
            folder_window: None,
            folder_paths: [None, None],
            folder_recursive: true,
            folder_filter: "*.*".into(),
            folder_show_identical: true,
            folder_sort: crate::folder::SortColumn::default(),
            folder_sort_descending: false,
        }
    }
}

impl Preferences {
    pub fn editor_tab_size(&self) -> TabSize {
        TabSize {
            tab_size: self.tab_size,
            ..Default::default()
        }
    }
}

fn deserialize_tab_size<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<usize, D::Error> {
    let size = usize::deserialize(deserializer)?;
    match size {
        2 | 4 => Ok(size),
        // Restore the default for the removed option without resetting other preferences.
        8 => Ok(4),
        _ => Err(serde::de::Error::custom("tab_size must be 2 or 4")),
    }
}
impl Global for Preferences {}

#[derive(Clone, Copy)]
pub enum WindowKind {
    Diff,
    Merge,
    Folder,
}

impl WindowKind {
    pub fn min_size(self) -> Size<Pixels> {
        match self {
            Self::Diff => size(px(1040.), px(640.)),
            Self::Merge => size(px(1160.), px(720.)),
            Self::Folder => size(px(1040.), px(640.)),
        }
    }

    pub fn bounds(self, cx: &App) -> WindowBounds {
        let preferences = cx.global::<Preferences>();
        let saved = match self {
            Self::Diff => preferences.diff_window,
            Self::Merge => preferences.merge_window,
            Self::Folder => preferences.folder_window,
        };
        let displays: Vec<_> = cx
            .displays()
            .iter()
            .map(|display| display.bounds())
            .collect();
        if let Some(bounds) = saved.and_then(|saved| saved.restore(self.min_size(), &displays)) {
            return bounds;
        }
        let default_size = match self {
            Self::Diff => size(px(1360.), px(860.)),
            Self::Merge => size(px(1480.), px(920.)),
            Self::Folder => size(px(1360.), px(860.)),
        };
        WindowBounds::centered(default_size, cx)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WindowMode {
    #[default]
    Windowed,
    Maximized,
    Fullscreen,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedWindow {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    #[serde(default)]
    mode: WindowMode,
}

impl From<WindowBounds> for SavedWindow {
    fn from(bounds: WindowBounds) -> Self {
        let mode = match bounds {
            WindowBounds::Windowed(_) => WindowMode::Windowed,
            WindowBounds::Maximized(_) => WindowMode::Maximized,
            WindowBounds::Fullscreen(_) => WindowMode::Fullscreen,
        };
        // GPUI supplies the normal restore bounds even while maximized/fullscreen.
        let bounds = bounds.get_bounds();
        Self {
            x: bounds.origin.x.into(),
            y: bounds.origin.y.into(),
            width: bounds.size.width.into(),
            height: bounds.size.height.into(),
            mode,
        }
    }
}

impl SavedWindow {
    fn restore(self, minimum: Size<Pixels>, displays: &[Bounds<Pixels>]) -> Option<WindowBounds> {
        if ![self.x, self.y, self.width, self.height]
            .into_iter()
            .all(f32::is_finite)
            || self.width <= 0.
            || self.height <= 0.
        {
            return None;
        }
        let mut bounds = Bounds::new(
            point(px(self.x), px(self.y)),
            size(
                px(self.width).max(minimum.width),
                px(self.height).max(minimum.height),
            ),
        );
        // Prefer the display containing the largest part of the old window. If
        // it was disconnected, move the entire window onto an available display.
        if let Some(display) = displays.iter().max_by(|a, b| {
            let overlap = |display: &Bounds<Pixels>| {
                let width = (bounds.right().min(display.right())
                    - bounds.left().max(display.left()))
                .max(px(0.));
                let height = (bounds.bottom().min(display.bottom())
                    - bounds.top().max(display.top()))
                .max(px(0.));
                f32::from(width) * f32::from(height)
            };
            overlap(a).total_cmp(&overlap(b))
        }) {
            bounds.size.width = bounds.size.width.min(display.size.width);
            bounds.size.height = bounds.size.height.min(display.size.height);
            bounds.origin.x = bounds
                .origin
                .x
                .clamp(display.left(), display.right() - bounds.size.width);
            bounds.origin.y = bounds
                .origin
                .y
                .clamp(display.top(), display.bottom() - bounds.size.height);
        }
        Some(match self.mode {
            WindowMode::Windowed => WindowBounds::Windowed(bounds),
            WindowMode::Maximized => WindowBounds::Maximized(bounds),
            WindowMode::Fullscreen => WindowBounds::Fullscreen(bounds),
        })
    }
}

fn path() -> Result<PathBuf, String> {
    #[cfg(windows)]
    let folder = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(target_os = "macos")]
    let folder = std::env::var_os("HOME")
        .map(|home| PathBuf::from(home).join("Library/Application Support"));
    #[cfg(not(any(windows, target_os = "macos")))]
    let folder = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")));
    folder
        .map(|folder| folder.join("rdiff/settings.json"))
        .ok_or_else(|| "Settings directory is unavailable".into())
}

fn save_to(path: &std::path::Path, preferences: &Preferences) -> Result<(), String> {
    let parent = path.parent().ok_or("Settings directory is unavailable")?;
    std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|error| error.to_string())?;
    file.write_all(&serde_json::to_vec_pretty(preferences).map_err(|error| error.to_string())?)
        .map_err(|error| error.to_string())?;
    file.as_file()
        .sync_all()
        .map_err(|error| error.to_string())?;
    file.persist(path).map_err(|error| error.to_string())?;
    Ok(())
}

pub fn init(cx: &mut App) {
    let preferences = path()
        .ok()
        .and_then(|path| std::fs::read(path).ok())
        .and_then(|bytes| serde_json::from_slice::<Preferences>(&bytes).ok())
        .unwrap_or_default();
    cx.set_global(preferences);
    apply(cx);
}

fn save_change(
    path: &std::path::Path,
    fallback: &Preferences,
    change: impl FnOnce(&mut Preferences),
) -> Result<(), String> {
    // Other rdiff processes may have saved unrelated options since launch.
    let mut preferences = std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Preferences>(&bytes).ok())
        .unwrap_or_else(|| fallback.clone());
    change(&mut preferences);
    save_to(path, &preferences)
}

fn update_preferences(change: impl Fn(&mut Preferences), cx: &mut App) -> Result<(), String> {
    let mut preferences = cx.global::<Preferences>().clone();
    let result = path().and_then(|path| save_change(&path, &preferences, &change));
    // Keep this window's other active preferences stable even if another
    // process has saved a different theme or comparison mode in the meantime.
    change(&mut preferences);
    cx.set_global(preferences);
    result
}

fn apply(cx: &mut App) {
    let preferences = cx.global::<Preferences>().clone();
    crate::i18n::set_language(preferences.language);
    gpui_kit::component::set_locale(if crate::i18n::korean() { "ko" } else { "en" });
    let mode = match preferences.theme {
        Appearance::Light => ThemeMode::Light,
        Appearance::Dark => ThemeMode::Dark,
        Appearance::System => cx.window_appearance().into(),
    };
    DARK.store(mode.is_dark(), Ordering::Relaxed);
    Theme::change(mode, None, cx);
    crate::surface::configure_theme(cx);
    cx.refresh_windows();
}

pub fn dark() -> bool {
    DARK.load(Ordering::Relaxed)
}
pub fn popup_open() -> bool {
    POPUPS.load(Ordering::Relaxed) != 0
}

pub fn popover(id: &'static str) -> Popover {
    let bit = match id {
        "comparison_menu" => 1,
        "actions_menu" => 2,
        "settings-menu" => 4,
        _ => unreachable!("unknown app menu"),
    };
    Popover::new(id).on_open_change(move |open, _, _| {
        if *open {
            POPUPS.fetch_or(bit, Ordering::Relaxed);
        } else {
            POPUPS.fetch_and(!bit, Ordering::Relaxed);
        }
    })
}

pub fn icon(name: &str) -> Icon {
    Icon::default().path(format!("icons/{name}.svg")).small()
}

pub fn color(value: u32) -> u32 {
    if dark() {
        return value;
    }
    match value {
        0x1e1f22 => 0xffffff,
        0x2b2d30 => 0xf3f4f6,
        0x393b40 => 0xd7dae0,
        0x8491a3 | 0x92979f => 0x626b78,
        0xd6dfeb | 0xbcbec4 => 0x242830,
        0x83d6ad => 0x237045,
        0xf0a1a8 => 0xb23949,
        0xe2c88a | 0xd9bc7c => 0x85620e,
        0x606366 | 0x606a79 => 0x737986,
        0xa4a7ad => 0x303844,
        0x365880 => 0xb8d5f5,
        0x242629 => 0xf0f1f4,
        0x7a7e85 => 0x8a909a,
        0xd2d6dc => 0x515968,
        0x55585e => 0xaeb3bc,
        0x42272f | 0x2d2027 => 0xffe8eb,
        0x1c3c31 | 0x192d26 => 0xe0f2e5,
        0x131820 => 0xf3f5f8,
        0x75404e => 0xf4bec8,
        0x32674f => 0xb8e0c6,
        0x32455b => 0xdceaf9,
        0x3b3423 => 0xfff4c2,
        0xffffff => 0x202630,
        _ => value,
    }
}

pub struct SettingsPanel {
    updater: Entity<crate::update_ui::UpdateControl>,
    error: Option<String>,
    _appearance: Subscription,
    kind: WindowKind,
    pending_window: Option<SavedWindow>,
    window_save: Option<Task<()>>,
    _bounds: Subscription,
    _release: Subscription,
}

impl SettingsPanel {
    pub fn new(kind: WindowKind, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let appearance = cx.observe_window_appearance(window, |_, _, cx| {
            if cx.global::<Preferences>().theme == Appearance::System {
                apply(cx);
            }
        });
        let bounds = cx.observe_window_bounds(window, |this, window, cx| {
            this.pending_window = Some(window.window_bounds().into());
            this.window_save = Some(cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(300))
                    .await;
                let _ = this.update(cx, |this, cx| {
                    this.persist_window(cx);
                    cx.notify();
                });
            }));
        });
        let release = cx.on_release(|this, cx| this.persist_window(cx));
        Self {
            updater: cx.new(|_| crate::update_ui::UpdateControl::new()),
            error: None,
            _appearance: appearance,
            kind,
            pending_window: Some(window.window_bounds().into()),
            window_save: None,
            _bounds: bounds,
            _release: release,
        }
    }

    fn change(&mut self, change: impl Fn(&mut Preferences), cx: &mut Context<Self>) {
        self.remember(change, cx);
        apply(cx);
    }

    pub fn remember(&mut self, change: impl Fn(&mut Preferences), cx: &mut Context<Self>) {
        self.error = update_preferences(change, cx).err();
        cx.notify();
    }

    fn persist_window(&mut self, cx: &mut App) {
        let Some(saved) = self.pending_window.take() else {
            return;
        };
        let kind = self.kind;
        self.error = update_preferences(
            |preferences| match kind {
                WindowKind::Diff => preferences.diff_window = Some(saved),
                WindowKind::Merge => preferences.merge_window = Some(saved),
                WindowKind::Folder => preferences.folder_window = Some(saved),
            },
            cx,
        )
        .err();
    }
}

impl Render for SettingsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let preferences = cx.global::<Preferences>().clone();
        div()
            .w(px(300.))
            .p_3()
            .flex()
            .flex_col()
            .gap_3()
            .child(div().font_weight(FontWeight::MEDIUM).child(tr("Settings")))
            .child(div().text_sm().child(tr("Language")))
            .child(
                div().flex().gap_1().children(
                    [
                        (Language::System, "System default"),
                        (Language::Ko, "한국어"),
                        (Language::En, "English"),
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(id, (language, label))| {
                        Button::new(("language", id))
                            .small()
                            .ghost()
                            .label(tr(label))
                            .selected(preferences.language == language)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.change(|preferences| preferences.language = language, cx)
                            }))
                    }),
                ),
            )
            .child(div().text_sm().child(tr("Appearance")))
            .child(
                div().flex().gap_1().children(
                    [
                        (Appearance::Light, "Light"),
                        (Appearance::Dark, "Dark"),
                        (Appearance::System, "System"),
                    ]
                    .into_iter()
                    .enumerate()
                    .map(|(id, (theme, label))| {
                        Button::new(("theme", id))
                            .small()
                            .ghost()
                            .label(tr(label))
                            .selected(preferences.theme == theme)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.change(|preferences| preferences.theme = theme, cx)
                            }))
                    }),
                ),
            )
            .child(div().text_sm().child(tr("Tab width")))
            .child(div().flex().gap_1().children([2, 4].map(|size| {
                Button::new(("tab-size", size))
                    .small()
                    .ghost()
                    .label(size.to_string())
                    .accessibility_label(format!("{}: {size}", tr("Tab width")))
                    .selected(preferences.tab_size == size)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.change(|preferences| preferences.tab_size = size, cx)
                    }))
            })))
            .child(div().h(px(1.)).bg(rgb(color(0x393b40))))
            .child(self.updater.clone())
            .children(self.error.as_ref().map(|error| {
                div()
                    .text_sm()
                    .text_color(rgb(color(0xf0a1a8)))
                    .child(format!("{}: {error}", tr("Could not save settings")))
            }))
    }
}

#[cfg(test)]
mod tests {
    use super::{Appearance, Language, Preferences, SavedWindow, WindowKind, save_change, save_to};
    use crate::diff::{HighlightMode, Whitespace};
    use gpui_kit::{Bounds, WindowBounds, point, px, size};
    #[test]
    fn settings_round_trip_and_defaults() {
        assert_eq!(
            serde_json::from_str::<Preferences>("{}").unwrap(),
            Preferences::default()
        );
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        for theme in [Appearance::Light, Appearance::Dark, Appearance::System] {
            let preferences = Preferences {
                language: Language::Ko,
                theme,
                ..Default::default()
            };
            save_to(&path, &preferences).unwrap();
            assert_eq!(
                serde_json::from_slice::<Preferences>(&std::fs::read(&path).unwrap()).unwrap(),
                preferences
            );
        }
        assert!(serde_json::from_str::<Preferences>(r#"{"theme":"invalid"}"#).is_err());
    }

    #[test]
    fn old_settings_keep_language_and_theme_with_default_tab_width() {
        let preferences: Preferences =
            serde_json::from_str(r#"{"language":"ko","theme":"dark"}"#).unwrap();
        assert_eq!(preferences.language, Language::Ko);
        assert_eq!(preferences.theme, Appearance::Dark);
        assert_eq!(preferences.tab_size, 4);
        assert_eq!(
            preferences,
            Preferences {
                language: Language::Ko,
                theme: Appearance::Dark,
                ..Default::default()
            }
        );
    }

    #[test]
    fn all_options_and_both_window_placements_survive_repeated_saves() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let bounds = Bounds::new(point(px(80.), px(40.)), size(px(1280.), px(800.)));
        for highlight in [
            HighlightMode::Words,
            HighlightMode::Characters,
            HighlightMode::Lines,
            HighlightMode::None,
        ] {
            for whitespace in [Whitespace::Exact, Whitespace::Trim, Whitespace::Ignore] {
                let preferences = Preferences {
                    language: Language::Ko,
                    theme: Appearance::Dark,
                    tab_size: 2,
                    unified: true,
                    whitespace,
                    highlight,
                    sync_scroll: false,
                    curved: false,
                    collapse_unchanged: false,
                    show_changes: true,
                    show_conflicts: true,
                    show_base: true,
                    diff_window: Some(WindowBounds::Windowed(bounds).into()),
                    merge_window: Some(WindowBounds::Maximized(bounds).into()),
                    folder_window: Some(WindowBounds::Windowed(bounds).into()),
                    folder_paths: [Some("C:/원본".into()), Some("C:/수정".into())],
                    folder_recursive: false,
                    folder_filter: "*.h;*.cpp".into(),
                    folder_show_identical: false,
                    folder_sort: crate::folder::SortColumn::Status,
                    folder_sort_descending: true,
                };
                save_to(&path, &preferences).unwrap();
                let restored: Preferences =
                    serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                assert_eq!(restored, preferences);
                // Whitespace restoration must affect the very first comparison.
                let comparison =
                    crate::diff::compare_with_whitespace("  a b  \n", "ab\n", restored.whitespace);
                assert_eq!(
                    comparison.hunks.is_empty(),
                    whitespace == Whitespace::Ignore
                );
            }
        }
    }

    #[test]
    fn window_modes_preserve_normal_size_and_negative_monitor_coordinates() {
        let display = Bounds::new(point(px(-1920.), px(0.)), size(px(1920.), px(1080.)));
        let bounds = Bounds::new(point(px(-1800.), px(80.)), size(px(1280.), px(800.)));
        for window in [
            WindowBounds::Windowed(bounds),
            WindowBounds::Maximized(bounds),
            WindowBounds::Fullscreen(bounds),
        ] {
            let saved = SavedWindow::from(window);
            let json = serde_json::to_vec(&saved).unwrap();
            let restored: SavedWindow = serde_json::from_slice(&json).unwrap();
            assert_eq!(
                restored.restore(WindowKind::Diff.min_size(), &[display]),
                Some(window)
            );
        }
    }

    #[test]
    fn window_placement_stays_visible_after_display_changes() {
        let display = Bounds::new(point(px(0.), px(0.)), size(px(1920.), px(1080.)));
        let offscreen = Bounds::new(point(px(4000.), px(-2000.)), size(px(1280.), px(800.)));
        let restored = SavedWindow::from(WindowBounds::Maximized(offscreen))
            .restore(WindowKind::Merge.min_size(), &[display])
            .unwrap();
        assert_eq!(
            restored,
            WindowBounds::Maximized(Bounds::new(point(px(640.), px(0.)), offscreen.size))
        );

        let huge = Bounds::new(point(px(-200.), px(-100.)), size(px(8000.), px(4000.)));
        assert_eq!(
            SavedWindow::from(WindowBounds::Windowed(huge))
                .restore(WindowKind::Diff.min_size(), &[display]),
            Some(WindowBounds::Windowed(display))
        );

        let small = Bounds::new(point(px(100.), px(100.)), size(px(200.), px(100.)));
        assert_eq!(
            SavedWindow::from(WindowBounds::Windowed(small))
                .restore(WindowKind::Merge.min_size(), &[display])
                .unwrap()
                .get_bounds()
                .size,
            WindowKind::Merge.min_size()
        );
    }

    #[test]
    fn invalid_window_sizes_are_not_restored() {
        let window = SavedWindow::from(WindowBounds::Windowed(Bounds::new(
            point(px(0.), px(0.)),
            size(px(1280.), px(800.)),
        )));
        for saved in [
            SavedWindow {
                width: 0.,
                ..window
            },
            SavedWindow {
                height: -1.,
                ..window
            },
            SavedWindow {
                x: f32::NAN,
                ..window
            },
            SavedWindow {
                width: f32::INFINITY,
                ..window
            },
        ] {
            assert_eq!(saved.restore(WindowKind::Diff.min_size(), &[]), None);
        }
    }

    #[test]
    fn saving_one_option_or_window_keeps_changes_from_other_processes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let stale = Preferences::default();
        let other_window = Preferences {
            theme: Appearance::Dark,
            tab_size: 2,
            show_base: true,
            ..Default::default()
        };
        save_to(&path, &other_window).unwrap();
        save_change(&path, &stale, |preferences| preferences.unified = true).unwrap();
        let placement = WindowBounds::Maximized(Bounds::new(
            point(px(50.), px(50.)),
            size(px(1280.), px(800.)),
        ))
        .into();
        save_change(&path, &stale, |preferences| {
            preferences.diff_window = Some(placement)
        })
        .unwrap();
        let restored: Preferences = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            restored,
            Preferences {
                unified: true,
                diff_window: Some(placement),
                ..other_window
            }
        );
    }

    #[test]
    fn tab_width_is_saved_and_validated() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        for tab_size in [2, 4] {
            let preferences = Preferences {
                tab_size,
                ..Default::default()
            };
            save_to(&path, &preferences).unwrap();
            let restored: Preferences =
                serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            assert_eq!(restored, preferences);
            let tab = restored.editor_tab_size();
            assert_eq!(tab.tab_size, tab_size);
            assert!(!tab.hard_tabs);
        }
        let restored: Preferences =
            serde_json::from_str(r#"{"language":"ko","theme":"dark","tab_size":8}"#).unwrap();
        assert_eq!(restored.tab_size, 4);
        assert_eq!(restored.language, Language::Ko);
        assert_eq!(restored.theme, Appearance::Dark);
        for value in ["0", "1", "3", "16", "-1", "null", "\"4\""] {
            assert!(
                serde_json::from_str::<Preferences>(&format!(r#"{{"tab_size":{value}}}"#)).is_err()
            );
        }
    }
}
