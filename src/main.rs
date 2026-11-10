#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // no console window on Windows

mod chord_page;
mod live_page;

use chord_page::{ChordPage, Mode};
use eframe::egui;
use live_page::LivePage;
use open_one_piano::ble::{self, Command, Device, Event};
use open_one_piano::protocol::{LightColor, Tone};
use open_one_piano::tones::{self, Entry};
use std::sync::mpsc::Receiver;
use tokio::sync::mpsc::UnboundedSender;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Open ONE Piano")
            .with_app_id("open-one-piano")
            .with_inner_size([780.0, 580.0])
            .with_min_inner_size([560.0, 400.0])
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png"))
                    .expect("bundled icon is a valid PNG"),
            ),
        ..Default::default()
    };
    eframe::run_native(
        "Open ONE Piano",
        options,
        Box::new(|cc| Ok(Box::new(App::new(cc)))),
    )
}

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Tones,
    Chords,
    Live,
}

#[derive(PartialEq)]
enum Conn {
    Idle,
    Scanning,
    Connecting,
    Connected(String),
}

struct App {
    cmd: UnboundedSender<Command>,
    events: Receiver<Event>,
    conn: Conn,
    devices: Vec<Device>,
    last_device: Option<String>,
    /// Which tone list is shown, and whether the user picked it by hand.
    model: String,
    model_locked: bool,
    /// What the keyboard says it's playing.
    current: Option<Tone>,
    /// Highlighted in the list, applied with "Change tone".
    selected: Option<Tone>,
    /// Sent, waiting for the keyboard to confirm.
    pending: Option<Tone>,
    category: Option<String>,
    search: String,
    status: String,
    error: Option<String>,
    tab: Tab,
    chords: ChordPage,
    live: LivePage,
    /// Lights last sent to the keyboard, so we only send changes.
    sent_lights: Vec<(u8, LightColor)>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let (ev_tx, ev_rx) = std::sync::mpsc::channel();
        let ctx = cc.egui_ctx.clone();
        let cmd = ble::spawn(ev_tx, move || ctx.request_repaint());
        let storage = cc.storage;
        let get = |k: &str| storage.and_then(|s| eframe::get_value::<String>(s, k));
        let _ = cmd.send(Command::Scan);
        let mut chords = ChordPage::new(
            get("progression").unwrap_or_else(|| "C G Am F".into()),
            get("chord_base").and_then(|v| v.parse().ok()).unwrap_or(60),
            get("keys88").is_some_and(|v| v == "true"),
            get("highlight_enabled").is_some_and(|v| v == "true"),
        );
        if get("highlight_mode").as_deref() == Some("scale") {
            chords.mode = Mode::Scale;
        }
        chords.scale = get("scale").and_then(|v| v.parse().ok()).unwrap_or(0);
        chords.whole_keyboard = get("whole_keyboard").is_some_and(|v| v == "true");
        Self {
            cmd,
            events: ev_rx,
            conn: Conn::Scanning,
            devices: Vec::new(),
            last_device: get("last_device"),
            model: get("model").unwrap_or_else(|| "TOC".into()),
            model_locked: get("model_locked").is_some_and(|v| v == "true"),
            current: None,
            selected: None,
            pending: None,
            category: None,
            search: String::new(),
            status: "Looking for keyboards…".into(),
            error: None,
            tab: match get("tab").as_deref() {
                Some("chords") => Tab::Chords,
                Some("live") => Tab::Live,
                _ => Tab::Tones,
            },
            chords,
            live: LivePage::new(get("use_pedal").is_none_or(|v| v == "true")),
            sent_lights: Vec::new(),
        }
    }

    fn send(&self, c: Command) {
        let _ = self.cmd.send(c);
    }

    fn connected(&self) -> bool {
        matches!(self.conn, Conn::Connected(_))
    }

    fn list(&self) -> &'static [Entry] {
        tones::for_model(&self.model)
    }

    fn name_of(&self, tone: Tone) -> Option<&'static Entry> {
        self.list().iter().find(|e| e.tone == tone)
    }

    fn connect(&mut self, dev: Device) {
        self.conn = Conn::Connecting;
        self.error = None;
        if !self.model_locked
            && let Some(m) = tones::guess_from_name(&dev.name)
        {
            self.model = m.into();
        }
        self.send(Command::Connect(dev));
    }

    fn apply(&mut self, tone: Tone) {
        if self.connected() {
            self.pending = Some(tone);
            self.error = None;
            self.send(Command::SetTone(tone));
        }
    }

    fn handle_events(&mut self) {
        while let Ok(ev) = self.events.try_recv() {
            match ev {
                Event::Status(s) => self.status = s,
                Event::Found(dev) => {
                    let remembered = self.last_device.as_deref() == Some(dev.name.as_str());
                    self.devices.push(dev.clone());
                    if remembered && self.conn == Conn::Scanning {
                        self.connect(dev);
                    }
                }
                Event::ScanDone => {
                    if self.conn == Conn::Scanning {
                        self.conn = Conn::Idle;
                        match self.devices.len() {
                            0 => {
                                self.status =
                                    "No keyboard found. Turn it on, then click Scan.".into()
                            }
                            1 => {
                                let d = self.devices[0].clone();
                                self.connect(d);
                            }
                            _ => self.status = "Pick your keyboard.".into(),
                        }
                    }
                }
                Event::Connected(name) => {
                    self.sent_lights.clear(); // fresh connection: nothing lit yet
                    self.status = format!("Connected to {name}");
                    self.last_device = Some(name.clone());
                    self.conn = Conn::Connected(name);
                }
                Event::Settings(s) => {
                    if !self.model_locked
                        && let Some(m) = tones::detect(&self.model, s.main)
                    {
                        self.model = m.into();
                    }
                    if self.pending.is_some_and(|p| p != s.main) {
                        self.error = Some("The keyboard didn't accept that tone. Try another Tone list (next to the search box).".into());
                    }
                    self.current = Some(s.main);
                    self.pending = None;
                    if self.selected.is_none() {
                        self.selected = Some(s.main);
                    }
                }
                Event::Disconnected => {
                    if self.connected() {
                        self.status = "Disconnected".into();
                        self.conn = Conn::Idle;
                    }
                    self.current = None;
                    self.pending = None;
                    self.sent_lights.clear();
                    self.live.clear();
                }
                Event::Note { note, on } => self.live.on_note(note, on),
                Event::Pedal(down) => self.live.on_pedal(down),
                Event::Error(e) => {
                    if matches!(self.conn, Conn::Connecting | Conn::Scanning) {
                        self.conn = Conn::Idle;
                    }
                    self.pending = None;
                    self.error = Some(e);
                }
            }
        }
    }

    fn visible(&self) -> Vec<&'static Entry> {
        let q = self.search.to_lowercase();
        self.list()
            .iter()
            .filter(|e| self.category.as_ref().is_none_or(|c| &e.category == c))
            .filter(|e| {
                q.is_empty()
                    || e.name.to_lowercase().contains(&q)
                    || e.category.to_lowercase().contains(&q)
            })
            .collect()
    }

    /// Light the current highlight's keys whenever highlighting is enabled, on any tab.
    fn sync_lights(&mut self) {
        let want: Vec<(u8, LightColor)> = if self.connected() && self.chords.enabled {
            self.chords
                .notes()
                .into_iter()
                .map(|n| {
                    (
                        n,
                        if self.chords.chord.is_root(n) {
                            LightColor::Blue
                        } else {
                            LightColor::Red
                        },
                    )
                })
                .collect()
        } else {
            Vec::new()
        };
        if want != self.sent_lights && self.connected() {
            self.send(Command::Lights(want.clone()));
            self.sent_lights = want;
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| match &self.conn {
            Conn::Connected(name) => {
                ui.label(
                    egui::RichText::new(format!("Connected: {name}"))
                        .color(egui::Color32::from_rgb(70, 180, 100)),
                );
                if ui.button("Disconnect").clicked() {
                    self.send(Command::Disconnect);
                }
            }
            Conn::Scanning | Conn::Connecting => {
                ui.spinner();
                ui.label(&self.status);
            }
            Conn::Idle => {
                let mut pick = None;
                egui::ComboBox::from_id_salt("device")
                    .selected_text(if self.devices.is_empty() {
                        "No keyboards found"
                    } else {
                        "Choose a keyboard"
                    })
                    .show_ui(ui, |ui| {
                        for d in &self.devices {
                            if ui.selectable_label(false, &d.name).clicked() {
                                pick = Some(d.clone());
                            }
                        }
                    });
                if let Some(d) = pick {
                    self.connect(d);
                }
                if ui.button("Scan").clicked() {
                    self.devices.clear();
                    self.conn = Conn::Scanning;
                    self.error = None;
                    self.send(Command::Scan);
                }
                ui.label(&self.status);
            }
        });
        if let Some(e) = &self.error {
            ui.colored_label(egui::Color32::from_rgb(220, 90, 80), e);
        }
    }

    fn now_playing(&mut self, ui: &mut egui::Ui) {
        let connected = self.connected();
        egui::Frame::group(ui.style())
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.weak("NOW PLAYING");
                        let (title, sub) = match self.current {
                            None if connected => ("Reading…".to_string(), String::new()),
                            None => ("Not connected".to_string(), String::new()),
                            Some(t) => match self.name_of(t) {
                                Some(e) => (e.name.clone(), e.category.clone()),
                                None => (
                                    format!("Program {}", t.prog + 1),
                                    format!("bank {}/{}", t.msb, t.lsb),
                                ),
                            },
                        };
                        ui.label(egui::RichText::new(title).size(24.0).strong());
                        if !sub.is_empty() {
                            ui.weak(sub);
                        }
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let target = self.selected.filter(|s| Some(*s) != self.current);
                        let label = match (self.pending, target.and_then(|t| self.name_of(t))) {
                            (Some(_), _) => "Changing…".to_string(),
                            (None, Some(e)) => format!("Change tone to {}", e.name),
                            (None, None) => "Change tone".to_string(),
                        };
                        let button = egui::Button::new(egui::RichText::new(label).size(16.0))
                            .min_size(egui::vec2(180.0, 36.0));
                        let enabled = connected && target.is_some() && self.pending.is_none();
                        if ui.add_enabled(enabled, button).clicked()
                            && let Some(t) = target
                        {
                            self.apply(t);
                        }
                    });
                });
            });
    }
}

impl eframe::App for App {
    fn logic(&mut self, _ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_events();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("top").show(ui, |ui| {
            ui.add_space(6.0);
            self.top_bar(ui);
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                for (tab, label) in [
                    (Tab::Tones, "Tones"),
                    (Tab::Chords, "Highlight"),
                    (Tab::Live, "Live"),
                ] {
                    let text = egui::RichText::new(label).size(16.0);
                    if ui
                        .add(
                            egui::Button::selectable(self.tab == tab, text)
                                .min_size(egui::vec2(90.0, 28.0)),
                        )
                        .clicked()
                    {
                        self.tab = tab;
                    }
                }
                if self.chords.enabled && self.tab != Tab::Chords {
                    ui.add_space(8.0);
                    ui.weak(format!("Highlighting {}", self.chords.title()));
                    if ui.small_button("Turn off").clicked() {
                        self.chords.enabled = false;
                    }
                }
            });
            ui.add_space(4.0);
        });

        if self.tab == Tab::Chords {
            let connected = self.connected();
            egui::CentralPanel::default().show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.chords.show(ui, connected));
            });
            self.sync_lights();
            return;
        }
        if self.tab == Tab::Live {
            let connected = self.connected();
            let range = self.chords.range();
            egui::CentralPanel::default().show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.live.show(ui, range, connected));
            });
            self.sync_lights();
            return;
        }
        self.sync_lights();

        let mut categories: Vec<&str> = Vec::new();
        for e in self.list() {
            if !categories.contains(&e.category.as_str()) {
                categories.push(&e.category);
            }
        }
        egui::Panel::left("categories")
            .resizable(false)
            .default_size(170.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_space(6.0);
                    if ui
                        .selectable_label(self.category.is_none(), "All tones")
                        .clicked()
                    {
                        self.category = None;
                    }
                    for c in &categories {
                        let on = self.category.as_deref() == Some(*c);
                        if ui.selectable_label(on, *c).clicked() {
                            self.category = Some(c.to_string());
                        }
                    }
                });
            });

        let visible = self.visible();
        let mut apply = None;
        egui::CentralPanel::default().show(ui, |ui| {
            self.now_playing(ui);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text("Search tones…")
                        .desired_width(220.0),
                );
                let pos = self
                    .selected
                    .and_then(|s| visible.iter().position(|e| e.tone == s));
                ui.add_enabled_ui(!visible.is_empty(), |ui| {
                    if ui.button("< Prev").clicked() {
                        self.selected = Some(visible[pos.map_or(0, |p| p.saturating_sub(1))].tone);
                    }
                    if ui.button("Next >").clicked() {
                        self.selected =
                            Some(visible[pos.map_or(0, |p| (p + 1).min(visible.len() - 1))].tone);
                    }
                });
                ui.weak("Double-click or press Enter to change.");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let before = self.model.clone();
                    egui::ComboBox::from_id_salt("model").selected_text(&self.model).show_ui(ui, |ui| {
                        for m in tones::MODELS {
                            ui.selectable_value(&mut self.model, m.to_string(), m);
                        }
                    });
                    if self.model != before {
                        self.model_locked = true;
                        self.category = None;
                    }
                    ui.weak("Tone list").on_hover_text(
                        "Each ONE model has its own set of tones. This is picked automatically; change it only if tones don't apply.",
                    );
                });
            });
            ui.add_space(4.0);
            egui::ScrollArea::vertical().auto_shrink(false).show_rows(
                ui,
                22.0,
                visible.len(),
                |ui, range| {
                    for e in &visible[range] {
                        let is_current = self.current == Some(e.tone);
                        let text = if is_current {
                            format!("{}   (playing)", e.name)
                        } else {
                            e.name.clone()
                        };
                        let text = if is_current {
                            egui::RichText::new(text).strong()
                        } else {
                            egui::RichText::new(text)
                        };
                        let r = ui.add(
                            egui::Button::selectable(self.selected == Some(e.tone), text)
                                .min_size(egui::vec2(ui.available_width(), 20.0)),
                        );
                        let r = if self.category.is_none() {
                            r.on_hover_text(&e.category)
                        } else {
                            r
                        };
                        if r.clicked() {
                            self.selected = Some(e.tone);
                        }
                        if r.double_clicked() {
                            apply = Some(e.tone);
                        }
                    }
                },
            );
        });

        let enter = ui.ctx().input(|i| i.key_pressed(egui::Key::Enter));
        // (Tones tab only: the Chords tab returned above.)
        if enter && let Some(s) = self.selected.filter(|s| Some(*s) != self.current) {
            apply = Some(s);
        }
        if let Some(t) = apply {
            self.apply(t);
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        if let Some(d) = &self.last_device {
            eframe::set_value(storage, "last_device", d);
        }
        eframe::set_value(storage, "model", &self.model);
        eframe::set_value(storage, "model_locked", &self.model_locked.to_string());
        let tab = match self.tab {
            Tab::Tones => "tones",
            Tab::Chords => "chords",
            Tab::Live => "live",
        };
        eframe::set_value(storage, "tab", &tab.to_string());
        eframe::set_value(
            storage,
            "highlight_mode",
            &(if self.chords.mode == Mode::Scale {
                "scale"
            } else {
                "chord"
            })
            .to_string(),
        );
        eframe::set_value(storage, "scale", &self.chords.scale.to_string());
        eframe::set_value(
            storage,
            "whole_keyboard",
            &self.chords.whole_keyboard.to_string(),
        );
        eframe::set_value(storage, "use_pedal", &self.live.use_pedal.to_string());
        eframe::set_value(storage, "progression", &self.chords.prog_text);
        eframe::set_value(storage, "chord_base", &self.chords.base.to_string());
        eframe::set_value(storage, "keys88", &self.chords.keys88.to_string());
        eframe::set_value(
            storage,
            "highlight_enabled",
            &self.chords.enabled.to_string(),
        );
    }
}
