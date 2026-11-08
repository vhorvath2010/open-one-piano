//! The "Live" page: show the keys being played and name the chord they make.

use crate::chord_page::draw_keyboard;
use eframe::egui::{self, Color32, RichText, vec2};
use open_one_piano::chords::{self, Chord};
use std::collections::BTreeSet;

pub struct LivePage {
    /// Keys physically held down.
    held: BTreeSet<u8>,
    /// Keys released while the sustain pedal was down (still sounding).
    sustained: BTreeSet<u8>,
    pedal: bool,
    pub use_pedal: bool,
    /// The chord being formed; written to the history when keys start lifting.
    pending: Option<Chord>,
    /// Shown while no keys are down, so the name doesn't vanish the moment you let go.
    last: Option<Chord>,
    history: Vec<Chord>,
}

impl LivePage {
    pub fn new(use_pedal: bool) -> Self {
        LivePage {
            held: BTreeSet::new(),
            sustained: BTreeSet::new(),
            pedal: false,
            use_pedal,
            pending: None,
            last: None,
            history: Vec::new(),
        }
    }

    /// Notes that are sounding: held keys, plus pedal-sustained ones if enabled.
    fn sounding(&self) -> Vec<u8> {
        let mut v: BTreeSet<u8> = self.held.clone();
        if self.use_pedal {
            v.extend(&self.sustained);
        }
        v.into_iter().collect()
    }

    pub fn on_note(&mut self, note: u8, on: bool) {
        let before = self.sounding().len();
        if on {
            self.held.insert(note);
            self.sustained.remove(&note);
        } else {
            self.held.remove(&note);
            if self.pedal {
                self.sustained.insert(note);
            }
        }
        self.after_change(before);
    }

    pub fn on_pedal(&mut self, down: bool) {
        let before = self.sounding().len();
        self.pedal = down;
        if !down {
            self.sustained.clear();
        }
        self.after_change(before);
    }

    pub fn clear(&mut self) {
        self.held.clear();
        self.sustained.clear();
        self.pedal = false;
        self.pending = None;
    }

    fn after_change(&mut self, before: usize) {
        let now = self.sounding();
        if now.len() >= before {
            // Building up a chord: remember the latest name it has.
            if let Some(c) = chords::identify(&now) {
                self.pending = Some(c);
                self.last = Some(c);
            }
        } else if let Some(c) = self.pending.take() {
            // Keys lifting: the chord is done. Log it unless it repeats the previous one.
            if self.history.last() != Some(&c) {
                self.history.push(c);
                if self.history.len() > 24 {
                    self.history.remove(0);
                }
            }
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, range: (u8, u8), connected: bool) {
        let notes = self.sounding();
        let current = chords::identify(&notes);

        egui::Frame::group(ui.style()).inner_margin(12.0).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    let (title, sub, weak) = match (notes.len(), current) {
                        (_, Some(c)) => (c.symbol(), c.long_name(), false),
                        (0, None) => match self.last {
                            Some(c) => (c.symbol(), c.long_name(), true),
                            None => (
                                "Play something".to_string(),
                                if connected {
                                    "Chords you play are named here.".to_string()
                                } else {
                                    "Connect your keyboard first.".to_string()
                                },
                                true,
                            ),
                        },
                        (1, None) => (chords::note_name(notes[0]), "Single note".to_string(), false),
                        (2, None) => (
                            format!("{} + {}", chords::note_name(notes[0]), chords::note_name(notes[1])),
                            chords::interval_name(notes[0], notes[1]).to_string(),
                            false,
                        ),
                        (_, None) => ("?".to_string(), "Not a chord I know".to_string(), false),
                    };
                    let title = RichText::new(title).size(34.0).strong();
                    ui.label(if weak { title.weak() } else { title });
                    ui.label(sub);
                    let names: Vec<String> = notes.iter().map(|&n| chords::note_name(n)).collect();
                    ui.weak(if names.is_empty() { " ".to_string() } else { names.join("  ") });
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.checkbox(&mut self.use_pedal, "Count notes held by the pedal")
                        .on_hover_text("With this on, notes you've let go of but are still ringing because of the sustain pedal count toward the chord.");
                    if self.pedal {
                        ui.colored_label(Color32::from_rgb(230, 170, 60), "Pedal");
                    }
                });
            });
        });
        ui.add_space(10.0);

        let root = current.map(|c| c.root);
        draw_keyboard(ui, range, &notes, |n| root == Some(n % 12));
        ui.add_space(12.0);

        ui.horizontal(|ui| {
            ui.label(RichText::new("What you played").strong());
            if ui
                .add_enabled(!self.history.is_empty(), egui::Button::new("Clear"))
                .clicked()
            {
                self.history.clear();
                self.last = None;
            }
        });
        ui.horizontal_wrapped(|ui| {
            if self.history.is_empty() {
                ui.weak("Chords you play will be listed here, in order.");
            }
            for c in &self.history {
                ui.add(
                    egui::Button::new(RichText::new(c.symbol()).size(16.0))
                        .min_size(vec2(44.0, 28.0)),
                )
                .on_hover_text(c.long_name());
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logs_chord_when_released() {
        let mut p = LivePage::new(true);
        for n in [60, 64, 67] {
            p.on_note(n, true);
        }
        assert!(p.history.is_empty());
        p.on_note(64, false);
        assert_eq!(
            p.history.iter().map(Chord::symbol).collect::<Vec<_>>(),
            ["C"]
        );
        for n in [60, 67] {
            p.on_note(n, false);
        }
        assert_eq!(p.history.len(), 1);
    }

    #[test]
    fn pedal_keeps_notes() {
        let mut p = LivePage::new(true);
        p.on_pedal(true);
        for n in [57, 60, 64] {
            p.on_note(n, true);
            p.on_note(n, false); // arpeggiated with the pedal down
        }
        assert_eq!(p.sounding(), vec![57, 60, 64]);
        assert_eq!(
            chords::identify(&p.sounding())
                .map(|c| c.symbol())
                .as_deref(),
            Some("Am")
        );
        p.on_pedal(false);
        assert!(p.sounding().is_empty());
        assert_eq!(
            p.history.iter().map(Chord::symbol).collect::<Vec<_>>(),
            ["Am"]
        );
    }
}
