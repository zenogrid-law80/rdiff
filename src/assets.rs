use gpui_kit::{AssetSource, Result, SharedString, assets};

assets::icon_assets!(ExtraIcons, [Save, SlidersHorizontal, List]);

pub struct AppAssets;

impl AssetSource for AppAssets {
    fn load(&self, path: &str) -> Result<Option<std::borrow::Cow<'static, [u8]>>> {
        if let Some(bytes) = ExtraIcons.load(path)? {
            return Ok(Some(bytes));
        }
        assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths = assets::Assets.list(path)?;
        paths.extend(ExtraIcons.list(path)?);
        paths.sort();
        paths.dedup();
        Ok(paths)
    }
}

#[cfg(test)]
mod tests {
    use super::{AppAssets, AssetSource};

    #[test]
    fn toolbar_icons_are_bundled() {
        for name in [
            "save",
            "sliders-horizontal",
            "list",
            "settings",
            "folder-open",
            "undo-2",
            "redo-2",
            "arrow-up",
            "arrow-down",
            "check",
            "ellipsis",
        ] {
            assert!(
                AppAssets
                    .load(&format!("icons/{name}.svg"))
                    .unwrap()
                    .is_some(),
                "{name}"
            );
        }
    }
}
