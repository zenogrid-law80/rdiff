use crate::settings::Language;
use std::{
    collections::HashMap,
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

static KOREAN: AtomicBool = AtomicBool::new(false);

pub fn korean() -> bool {
    KOREAN.load(Ordering::Relaxed)
}

pub fn set_language(language: Language) {
    KOREAN.store(
        match language {
            Language::System => system_korean(),
            Language::En => false,
            Language::Ko => true,
        },
        Ordering::Relaxed,
    );
}

fn system_korean() -> bool {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn GetUserDefaultUILanguage() -> u16;
        }
        unsafe { GetUserDefaultUILanguage() & 0x03ff == 0x12 }
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("/usr/bin/defaults")
            .args(["read", "-g", "AppleLanguages"])
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| String::from_utf8(output.stdout).ok())
            .and_then(|text| {
                text.lines()
                    .map(|line| line.trim().trim_matches(['"', ',', '(', ')', ' ']))
                    .find(|line| !line.is_empty())
                    .map(|line| line.starts_with("ko"))
            })
            .unwrap_or(false)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        ["LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .find_map(|key| std::env::var(key).ok().filter(|value| !value.is_empty()))
            .is_some_and(|value| value.starts_with("ko"))
    }
}

pub fn tr(text: impl AsRef<str>) -> String {
    translate(text.as_ref(), korean())
}

fn translate(text: &str, ko: bool) -> String {
    if !ko {
        return text.into();
    }
    static WORDS: OnceLock<HashMap<String, String>> = OnceLock::new();
    let words = WORDS.get_or_init(|| {
        serde_json::from_str(include_str!("../locales/ko.json")).expect("valid translation catalog")
    });
    if let Some(value) = words.get(text) {
        return value.clone();
    }
    if text.contains(" · ") {
        return text
            .split(" · ")
            .map(|part| translate(part, ko))
            .collect::<Vec<_>>()
            .join(" · ");
    }
    for (prefix, replacement) in [
        ("Update failed: ", "업데이트 실패: "),
        ("Update available: ", "새 버전이 있습니다: "),
        ("Save failed for ", "파일 저장 실패: "),
        ("Saved ", "저장 완료: "),
        ("Saving ", "저장 중: "),
        ("Could not open ", "파일 열기 실패: "),
        ("File picker failed: ", "파일 선택 실패: "),
        ("Install v", "설치 v"),
        ("MSI update queued for v", "MSI 업데이트 준비 완료 v"),
        ("Installed v", "설치 완료 v"),
    ] {
        if let Some(rest) = text.strip_prefix(prefix) {
            return format!("{replacement}{rest}");
        }
    }
    for (suffix, replacement) in [
        (" differences", "개 차이"),
        (" additions", "줄 추가"),
        (" CHANGE GROUPS", "개 변경 구간"),
        (" is up to date", "는 최신 버전입니다"),
        (" is up to date.", "는 최신 버전입니다."),
    ] {
        if let Some(rest) = text.strip_suffix(suffix) {
            return format!("{rest}{replacement}");
        }
    }
    text.into()
}

#[cfg(test)]
mod tests {
    use super::translate;
    #[test]
    fn translates_ui_without_changing_source_text() {
        assert_eq!(translate("Save", true), "저장");
        assert_eq!(translate("Save", false), "Save");
        assert_eq!(
            translate("Saved C:/src/파일.rs", true),
            "저장 완료: C:/src/파일.rs"
        );
        assert_eq!(translate("Undo · Ctrl+Z", true), "실행 취소 · Ctrl+Z");
        assert_eq!(translate("fn main() {}", true), "fn main() {}");
    }
}
