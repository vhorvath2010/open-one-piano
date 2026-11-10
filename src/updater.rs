//! Update banner: checks for a new release in the background and installs it on request.

use eframe::egui::{self, Color32, RichText};
use open_one_piano::update::{self, Release};
use std::sync::mpsc::{Receiver, channel};

enum State {
    Idle,
    Checking {
        manual: bool,
    },
    UpToDate,
    Available(Release),
    Installing(String),
    /// Installed this version; it runs after a restart.
    Installed(String),
    Failed(String),
    Dismissed,
}

pub struct Updater {
    state: State,
    rx: Option<Receiver<State>>,
    /// Relaunch the app once this one has closed.
    pub restart: bool,
}

impl Updater {
    pub fn new() -> Self {
        Updater {
            state: State::Idle,
            rx: None,
            restart: false,
        }
    }

    fn busy(&self) -> bool {
        matches!(self.state, State::Checking { .. } | State::Installing(_))
    }

    /// Look for a newer release on a background thread. A manual check also
    /// reports "up to date" and errors; an automatic one stays quiet about them.
    pub fn check(&mut self, ctx: &egui::Context, manual: bool) {
        if self.busy() {
            return;
        }
        self.state = State::Checking { manual };
        let (tx, rx) = channel();
        self.rx = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let next = match update::check() {
                Ok(Some(rel)) => State::Available(rel),
                Ok(None) if manual => State::UpToDate,
                Err(e) if manual => State::Failed(e),
                _ => State::Idle,
            };
            let _ = tx.send(next);
            ctx.request_repaint();
        });
    }

    fn install(&mut self, ctx: &egui::Context, rel: Release) {
        let Some(url) = rel.asset.clone() else { return };
        self.state = State::Installing(rel.version.clone());
        let (tx, rx) = channel();
        self.rx = Some(rx);
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let next = match update::download(&url).and_then(|exe| update::install(&exe)) {
                Ok(()) => State::Installed(rel.version),
                Err(e) => State::Failed(e),
            };
            let _ = tx.send(next);
            ctx.request_repaint();
        });
    }

    fn poll(&mut self) {
        if let Some(rx) = &self.rx
            && let Ok(s) = rx.try_recv()
        {
            self.state = s;
            self.rx = None;
        }
    }

    /// Version label and "Check for updates", for the right side of a row.
    pub fn version_button(&mut self, ui: &mut egui::Ui) {
        self.poll();
        let label = if matches!(self.state, State::Checking { manual: true }) {
            "Checking…"
        } else {
            "Check for updates"
        };
        if ui
            .add_enabled(!self.busy(), egui::Button::new(label).small())
            .clicked()
        {
            self.check(ui.ctx(), true);
        }
        ui.weak(format!("v{}", update::current_version()));
    }

    /// A one-line banner when there's something to say. Nothing otherwise.
    pub fn banner(&mut self, ui: &mut egui::Ui) {
        self.poll();
        let ctx = ui.ctx().clone();
        let green = Color32::from_rgb(70, 180, 100);
        let mut install = None;
        let mut dismiss = false;
        match &self.state {
            State::Idle | State::Checking { .. } | State::Dismissed => return,
            State::UpToDate => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("You're on the latest version.").color(green));
                    if ui.small_button("OK").clicked() {
                        dismiss = true;
                    }
                });
            }
            State::Available(rel) => {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(format!("Version {} is available.", rel.version))
                            .strong()
                            .color(Color32::from_rgb(110, 160, 255)),
                    );
                    if rel.asset.is_some() {
                        if ui.button("Update now").clicked() {
                            install = Some(rel.clone());
                        }
                        ui.hyperlink_to("What's new", &rel.page);
                    } else {
                        ui.hyperlink_to("Download", &rel.page);
                    }
                    if ui.small_button("Later").clicked() {
                        dismiss = true;
                    }
                });
            }
            State::Installing(v) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!("Downloading version {v}…"));
                });
            }
            State::Installed(v) => {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Version {v} is installed.")).color(green));
                    if ui.button("Restart now").clicked() {
                        self.restart = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    ui.weak("or it starts next time you open the app.");
                });
            }
            State::Failed(e) => {
                ui.horizontal(|ui| {
                    ui.colored_label(
                        Color32::from_rgb(220, 90, 80),
                        format!("Update failed: {e}"),
                    );
                    if ui.small_button("OK").clicked() {
                        dismiss = true;
                    }
                });
            }
        }
        if let Some(rel) = install {
            self.install(&ctx, rel);
        } else if dismiss {
            self.state = State::Dismissed;
        }
    }

    /// Call from `on_exit`: start the freshly installed version.
    pub fn relaunch_if_requested(&self) {
        if self.restart
            && let Ok(exe) = std::env::current_exe()
        {
            let _ = std::process::Command::new(exe).spawn();
        }
    }
}
