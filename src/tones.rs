//! Tone lists for each keyboard model, embedded at build time.

use crate::protocol::Tone;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Debug)]
pub struct Entry {
    pub category: String,
    pub name: String,
    pub tone: Tone,
}

type Row = (String, String, u8, u8, u8);

fn all() -> &'static HashMap<String, Vec<Entry>> {
    static LISTS: OnceLock<HashMap<String, Vec<Entry>>> = OnceLock::new();
    LISTS.get_or_init(|| {
        let raw: HashMap<String, Vec<Row>> =
            serde_json::from_str(include_str!("../assets/tones.json"))
                .expect("tones.json is valid");
        raw.into_iter()
            .map(|(model, rows)| {
                let list = rows
                    .into_iter()
                    .map(|(category, name, msb, lsb, prog)| Entry {
                        category,
                        name,
                        tone: Tone { msb, lsb, prog },
                    })
                    .collect();
                (model, list)
            })
            .collect()
    })
}

/// Tone list codes, in the order they're offered to the user.
pub const MODELS: [&str; 6] = ["TOC", "TOK", "TON", "TOP", "TOP3", "TOPP"];

/// Best guess at which tone list a keyboard uses, from the tone it's currently playing.
/// Each model uses its own bank numbers, so the current tone usually pins it down.
/// Prefers `current` when it fits, so a GM tone shared by several lists doesn't flip it.
pub fn detect(current: &str, playing: Tone) -> Option<&'static str> {
    let has = |m: &str| for_model(m).iter().any(|e| e.tone == playing);
    if has(current) {
        return None;
    }
    MODELS.into_iter().find(|m| has(m))
}

/// Guess from the Bluetooth name before any tone is known.
pub fn guess_from_name(name: &str) -> Option<&'static str> {
    name.to_lowercase().contains("color").then_some("TOC")
}

/// The tone list for a model (e.g. "TOC"), falling back to the generic list.
pub fn for_model(model: &str) -> &'static [Entry] {
    let lists = all();
    lists
        .get(model)
        .or_else(|| lists.get("TOP"))
        .map(Vec::as_slice)
        .unwrap_or(&[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_model_loads() {
        for m in ["TOC", "TOK", "TON", "TOP", "TOP3", "TOPP"] {
            assert!(!for_model(m).is_empty(), "{m}");
        }
    }

    #[test]
    fn detects_color_from_power_on_piano() {
        // A ONE Piano COLOR powers on playing bank 16/0 program 0, which only TOC has.
        assert_eq!(
            detect(
                "TON",
                Tone {
                    msb: 16,
                    lsb: 0,
                    prog: 0
                }
            ),
            Some("TOC")
        );
        // A plain GM tone that the current list also has keeps the current list.
        assert_eq!(
            detect(
                "TOC",
                Tone {
                    msb: 0,
                    lsb: 0,
                    prog: 40
                }
            ),
            None
        );
    }

    #[test]
    fn trumpet_on_ton() {
        let t = for_model("TON")
            .iter()
            .find(|e| e.name == "Trumpet")
            .unwrap();
        assert_eq!(
            t.tone,
            Tone {
                msb: 121,
                lsb: 0,
                prog: 56
            }
        );
    }
}
