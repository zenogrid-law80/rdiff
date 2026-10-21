use crate::update::{self, Available};
use gpui_kit::{
    component::{
        Disableable, Sizable,
        button::{Button, ButtonVariants},
    },
    *,
};

pub struct UpdateControl {
    available: Option<Available>,
    busy: bool,
    installed: bool,
    failed: bool,
    queued: bool,
    message: String,
}

impl UpdateControl {
    pub fn new() -> Self {
        Self {
            available: None,
            busy: false,
            installed: false,
            failed: false,
            queued: false,
            message: format!(
                "rdiff v{} · Check GitHub Releases for updates",
                update::VERSION
            ),
        }
    }

    fn activate(&mut self, cx: &mut Context<Self>) {
        if self.busy || self.installed {
            return;
        }
        self.busy = true;
        self.failed = false;
        let available = self.available.take();
        let installing = available.is_some();
        self.message = if installing {
            "Downloading and verifying update…"
        } else {
            "Checking GitHub Releases…"
        }
        .into();
        cx.notify();
        let task = cx.background_executor().spawn(async move {
            match available {
                Some(release) => update::install(&release).map(|message| (None, message)),
                None => update::check().map(|release| {
                    let message = match &release {
                        Some(r) => format!("v{} available · Click Install update. Open documents stay unchanged; applies on next launch.", r.version),
                        None => format!("rdiff v{} is up to date", update::VERSION),
                    };
                    (release, message)
                }),
            }
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                match result {
                    Ok((available, message)) => {
                        this.queued = message.starts_with("MSI update queued");
                        this.available = available;
                        this.message = message;
                        this.installed = installing;
                    }
                    Err(error) => {
                        this.failed = true;
                        this.message = format!("Update failed: {error} · Click to retry");
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

impl Render for UpdateControl {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        Button::new("github-update")
            .small()
            .ghost()
            .label(if self.busy {
                "Updating…".to_owned()
            } else if self.failed {
                "Update failed · Retry".into()
            } else if self.installed {
                if self.queued {
                    "Update ready · Close rdiff".into()
                } else {
                    "Updated · Next launch".into()
                }
            } else if let Some(release) = &self.available {
                format!("Install v{}", release.version)
            } else {
                format!("v{} · Check updates", update::VERSION)
            })
            .tooltip(self.message.clone())
            .disabled(self.busy || self.installed)
            .on_click(cx.listener(|this, _, _, cx| this.activate(cx)))
    }
}
