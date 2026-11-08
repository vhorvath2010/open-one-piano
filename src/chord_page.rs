//! The "Chords" page: pick or step through chords, see them on a drawn keyboard,
//! and light the matching keys on the real keyboard.

use eframe::egui::{
    self, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, vec2,
};
use open_one_piano::chords::{self, Chord, NOTE_NAMES, QUALITIES};

const ROOT_COLOR: Color32 = Color32::from_rgb(70, 130, 230);
const NOTE_COLOR: Color32 = Color32::from_rgb(225, 75, 65);

pub struct ChordPage {
    pub chord: Chord,
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
            chord: Chord::new(0, 0),
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
        self.chord.notes(self.base, self.range())
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
        if !typing {
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

        let notes = self.notes();

        // Big chord readout
        egui::Frame::group(ui.style())
            .inner_margin(12.0)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(self.chord.symbol()).size(30.0).strong());
                        ui.label(self.chord.long_name());
                        let names: Vec<&str> = notes
                            .iter()
                            .map(|&n| NOTE_NAMES[(n % 12) as usize])
                            .collect();
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
        });
        // Inversion, octave, keyboard size
        ui.horizontal_wrapped(|ui| {
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
            ui.add_space(16.0);
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

fn draw_keyboard(ui: &mut egui::Ui, (lo, hi): (u8, u8), lit: &[u8], is_root: impl Fn(u8) -> bool) {
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
