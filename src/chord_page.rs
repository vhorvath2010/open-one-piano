//! The "Highlight" page: pick a chord or a scale (or step through a progression),
//! see it on a drawn keyboard, and light the matching keys on the real keyboard.

use eframe::egui::{
    self, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, vec2,
};
use open_one_piano::chords::{self, Chord, NOTE_NAMES, QUALITIES, SCALES};

const ROOT_COLOR: Color32 = Color32::from_rgb(70, 130, 230);
const NOTE_COLOR: Color32 = Color32::from_rgb(225, 75, 65);

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Chord,
    Scale,
}

pub struct ChordPage {
    pub mode: Mode,
    pub chord: Chord,
    /// Index into SCALES for scale mode (the root is `chord.root`).
    pub scale: usize,
    /// Scale mode: light the scale on every key instead of one octave.
    pub whole_keyboard: bool,
    /// MIDI note of the C the chord is built from (60 = middle C).
    pub base: u8,
    pub keys88: bool,
    pub lights: bool,
    pub prog_text: String,
    prog: Vec<Chord>,
    bad: Vec<String>,
    index: Option<usize>,
}

impl ChordPage {
    pub fn new(prog_text: String, base: u8, keys88: bool, lights: bool) -> Self {
        let mut p = ChordPage {
            mode: Mode::Chord,
            chord: Chord::new(0, 0),
            scale: 0,
            whole_keyboard: false,
            base,
            keys88,
            lights,
            prog_text,
            prog: Vec::new(),
            bad: Vec::new(),
            index: None,
        };
        p.reparse();
        p
    }

    pub fn range(&self) -> (u8, u8) {
        if self.keys88 { (21, 108) } else { (36, 96) }
    }

    pub fn notes(&self) -> Vec<u8> {
        match self.mode {
            Mode::Chord => self.chord.notes(self.base, self.range()),
            Mode::Scale => chords::scale_notes(
                self.chord.root,
                &SCALES[self.scale.min(SCALES.len() - 1)],
                self.base,
                self.range(),
                self.whole_keyboard,
            ),
        }
    }

    fn reparse(&mut self) {
        let (prog, bad) = chords::parse_progression(&self.prog_text);
        self.prog = prog;
        self.bad = bad;
        self.index = self.index.filter(|&i| i < self.prog.len());
    }

    fn go_to(&mut self, i: usize) {
        if let Some(c) = self.prog.get(i) {
            let inversion = self.chord.inversion;
            self.chord = *c;
            self.chord.inversion = inversion % self.chord.q().intervals.len();
            self.index = Some(i);
        }
    }

    fn step(&mut self, delta: isize) {
        if self.prog.is_empty() {
            return;
        }
        let n = self.prog.len() as isize;
        let next = match self.index {
            None => 0,
            Some(i) => (i as isize + delta).rem_euclid(n),
        };
        self.go_to(next as usize);
    }

    pub fn show(&mut self, ui: &mut egui::Ui, connected: bool) {
        // Arrow keys step through the progression unless a text box has focus.
        let typing = ui.ctx().memory(|m| m.focused().is_some());
        if !typing && self.mode == Mode::Chord {
            let (l, r) = ui.ctx().input(|i| {
                (
                    i.key_pressed(egui::Key::ArrowLeft),
                    i.key_pressed(egui::Key::ArrowRight),
                )
            });
            if l {
                self.step(-1);
            }
            if r {
                self.step(1);
            }
        }

        ui.horizontal(|ui| {
            for (mode, label) in [(Mode::Chord, "Chord"), (Mode::Scale, "Scale")] {
                let b =
                    egui::Button::selectable(self.mode == mode, label).min_size(vec2(70.0, 24.0));
                if ui.add(b).clicked() {
                    self.mode = mode;
                }
            }
        });
        ui.add_space(4.0);

        let notes = self.notes();
        let scale = &SCALES[self.scale.min(SCALES.len() - 1)];
        let root_name = NOTE_NAMES[self.chord.root as usize];
        let (title, subtitle) = match self.mode {
            Mode::Chord => (self.chord.symbol(), self.chord.long_name()),
            Mode::Scale => (
                format!("{root_name} {}", scale.name),
                if self.whole_keyboard {
                    "Scale, whole keyboard".to_string()
                } else {
                    "Scale, one octave".to_string()
                },
            ),
        };
        let mut seen = Vec::new();
        for &n in &notes {
            if !seen.contains(&(n % 12)) {
                seen.push(n % 12);
            }
        }
        let names: Vec<&str> = seen.iter().map(|&pc| NOTE_NAMES[pc as usize]).collect();

        // Big readout
        egui::Frame::group(ui.style())
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(&title).size(30.0).strong());
                        ui.label(&subtitle);
                        ui.weak(names.join("  "));
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.checkbox(&mut self.lights, "Light up keys");
                        if !connected {
                            ui.weak("Not connected");
                        }
                    });
                });
            });
        ui.add_space(6.0);

        // Root
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Root").strong());
            ui.add_space(8.0);
            for (pc, name) in NOTE_NAMES.iter().enumerate() {
                let on = self.chord.root == pc as u8 && self.chord.bass.is_none();
                if ui
                    .add(egui::Button::selectable(on, *name).min_size(vec2(34.0, 24.0)))
                    .clicked()
                {
                    self.chord.root = pc as u8;
                    self.chord.bass = None;
                    self.index = None;
                }
            }
        });
        // Type
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Type").strong());
            ui.add_space(8.0);
            match self.mode {
                Mode::Chord => {
                    for (i, q) in QUALITIES.iter().enumerate() {
                        if ui
                            .add(egui::Button::selectable(self.chord.quality == i, q.name))
                            .clicked()
                        {
                            self.chord.quality = i;
                            self.chord.inversion %= q.intervals.len();
                            self.index = None;
                        }
                    }
                }
                Mode::Scale => {
                    for (i, sc) in SCALES.iter().enumerate() {
                        if ui
                            .add(egui::Button::selectable(self.scale == i, sc.name))
                            .clicked()
                        {
                            self.scale = i;
                        }
                    }
                }
            }
        });
        // Inversion, octave, keyboard size
        ui.horizontal_wrapped(|ui| {
            match self.mode {
                Mode::Chord => {
                    ui.label(RichText::new("Inversion").strong());
                    let n = self.chord.q().intervals.len();
                    for (i, label) in ["Root", "1st", "2nd", "3rd", "4th"]
                        .iter()
                        .enumerate()
                        .take(n)
                    {
                        if ui
                            .add(egui::Button::selectable(self.chord.inversion == i, *label))
                            .clicked()
                        {
                            self.chord.inversion = i;
                        }
                    }
                }
                Mode::Scale => {
                    ui.checkbox(&mut self.whole_keyboard, "Whole keyboard");
                }
            }
            ui.add_space(16.0);
            let octave_matters = !(self.mode == Mode::Scale && self.whole_keyboard);
            ui.add_enabled_ui(octave_matters, |ui| {
                ui.label(RichText::new("Octave").strong());
                let (lo, hi) = self.range();
                if ui
                    .add_enabled(self.base >= lo + 12, egui::Button::new("<"))
                    .clicked()
                {
                    self.base -= 12;
                }
                ui.label(format!("C{}", self.base / 12 - 1));
                if ui
                    .add_enabled(self.base + 12 <= hi - 12, egui::Button::new(">"))
                    .clicked()
                {
                    self.base += 12;
                }
            });
            ui.add_space(16.0);
            egui::ComboBox::from_id_salt("keys")
                .selected_text(if self.keys88 { "88 keys" } else { "61 keys" })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.keys88, false, "61 keys");
                    ui.selectable_value(&mut self.keys88, true, "88 keys");
                });
        });
        ui.add_space(8.0);

        draw_keyboard(ui, self.range(), &notes, |n| self.chord.is_root(n));
        ui.add_space(10.0);
        if self.mode == Mode::Scale {
            return;
        }

        // Progression
        ui.horizontal(|ui| {
            ui.label(RichText::new("Progression").strong());
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.prog_text)
                    .hint_text("e.g. C G Am F")
                    .desired_width(ui.available_width() - 150.0),
            );
            if edit.changed() {
                self.reparse();
            }
            ui.add_enabled_ui(!self.prog.is_empty(), |ui| {
                if ui.button("< Prev").clicked() {
                    self.step(-1);
                }
                if ui.button("Next >").clicked() {
                    self.step(1);
                }
            });
        });
        ui.horizontal_wrapped(|ui| {
            let mut go = None;
            for (i, c) in self.prog.iter().enumerate() {
                let on = self.index == Some(i);
                if ui
                    .add(
                        egui::Button::selectable(on, RichText::new(c.symbol()).size(16.0))
                            .min_size(vec2(44.0, 28.0)),
                    )
                    .clicked()
                {
                    go = Some(i);
                }
            }
            if let Some(i) = go {
                self.go_to(i);
            }
        });
        if !self.bad.is_empty() {
            ui.colored_label(
                Color32::from_rgb(220, 90, 80),
                format!("Couldn't read: {}", self.bad.join(", ")),
            );
        }
        ui.weak("Type chord names separated by spaces. Click one, or use the Left / Right arrow keys to step through.");
    }
}

fn is_black(n: u8) -> bool {
    matches!(n % 12, 1 | 3 | 6 | 8 | 10)
}

fn note_label(n: u8) -> String {
    format!("{}{}", NOTE_NAMES[(n % 12) as usize], n / 12 - 1)
}

pub fn draw_keyboard(
    ui: &mut egui::Ui,
    (lo, hi): (u8, u8),
    lit: &[u8],
    is_root: impl Fn(u8) -> bool,
) {
    let whites: Vec<u8> = (lo..=hi).filter(|&n| !is_black(n)).collect();
    let width = ui.available_width();
    let height = (width / whites.len() as f32 * 5.5).clamp(70.0, 130.0);
    let (rect, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    let p = ui.painter_at(rect);
    let kw = width / whites.len() as f32;
    let color_of = |n: u8| if is_root(n) { ROOT_COLOR } else { NOTE_COLOR };
    let font = egui::FontId::proportional((kw * 0.42).clamp(7.0, 12.0));

    for (i, &n) in whites.iter().enumerate() {
        let r = Rect::from_min_size(
            Pos2::new(rect.left() + i as f32 * kw, rect.top()),
            vec2(kw, height),
        );
        let on = lit.contains(&n);
        p.rect(
            r.shrink(0.5),
            CornerRadius::same(2),
            if on {
                color_of(n)
            } else {
                Color32::from_gray(245)
            },
            Stroke::new(1.0, Color32::from_gray(90)),
            StrokeKind::Inside,
        );
        if on || n % 12 == 0 {
            let text = if on {
                NOTE_NAMES[(n % 12) as usize].to_string()
            } else {
                note_label(n)
            };
            let color = if on {
                Color32::WHITE
            } else {
                Color32::from_gray(120)
            };
            p.text(
                Pos2::new(r.center().x, r.bottom() - 6.0),
                egui::Align2::CENTER_BOTTOM,
                text,
                font.clone(),
                color,
            );
        }
    }
    let bw = kw * 0.6;
    let bh = height * 0.62;
    let mut white_index = 0usize;
    for n in lo..=hi {
        if !is_black(n) {
            white_index += 1;
            continue;
        }
        let x = rect.left() + white_index as f32 * kw - bw / 2.0;
        let r = Rect::from_min_size(Pos2::new(x, rect.top()), vec2(bw, bh));
        let on = lit.contains(&n);
        p.rect_filled(
            r,
            CornerRadius::same(2),
            if on {
                color_of(n)
            } else {
                Color32::from_gray(30)
            },
        );
        if on {
            p.text(
                Pos2::new(r.center().x, r.bottom() - 4.0),
                egui::Align2::CENTER_BOTTOM,
                NOTE_NAMES[(n % 12) as usize],
                font.clone(),
                Color32::WHITE,
            );
        }
    }
}
