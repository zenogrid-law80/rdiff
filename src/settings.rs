use crate::i18n::tr;
use gpui_kit::{
    component::{
        Icon, Selectable, Sizable,
        button::{Button, ButtonVariants},
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

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub language: Language,
    pub theme: Appearance,
}
impl Global for Preferences {}

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
        0xffffff => 0x202630,
        _ => value,
    }
}

pub struct SettingsPanel {
    updater: Entity<crate::update_ui::UpdateControl>,
    error: Option<String>,
    _appearance: Subscription,
}

impl SettingsPanel {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let appearance = cx.observe_window_appearance(window, |_, _, cx| {
            if cx.global::<Preferences>().theme == Appearance::System {
                apply(cx);
            }
        });
        Self {
            updater: cx.new(|_| crate::update_ui::UpdateControl::new()),
            error: None,
            _appearance: appearance,
        }
    }

    fn change(&mut self, change: impl FnOnce(&mut Preferences), cx: &mut Context<Self>) {
        let mut preferences = cx.global::<Preferences>().clone();
        change(&mut preferences);
        self.error = path().and_then(|path| save_to(&path, &preferences)).err();
        cx.set_global(preferences);
        apply(cx);
        cx.notify();
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
    use super::{Appearance, Language, Preferences, save_to};
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
            };
            save_to(&path, &preferences).unwrap();
            assert_eq!(
                serde_json::from_slice::<Preferences>(&std::fs::read(&path).unwrap()).unwrap(),
                preferences
            );
        }
        assert!(serde_json::from_str::<Preferences>(r#"{"theme":"invalid"}"#).is_err());
    }
}
