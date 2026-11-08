//! Chord spelling: root + quality + inversion → MIDI notes, and parsing chord symbols like "F#m7/C#".

pub const NOTE_NAMES: [&str; 12] = [
    "C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B",
];

/// A chord quality: display name, symbol suffix, and semitones above the root.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quality {
    pub name: &'static str,
    pub suffix: &'static str,
    pub intervals: &'static [u8],
}

pub const QUALITIES: &[Quality] = &[
    Quality {
        name: "Major",
        suffix: "",
        intervals: &[0, 4, 7],
    },
    Quality {
        name: "Minor",
        suffix: "m",
        intervals: &[0, 3, 7],
    },
    Quality {
        name: "7",
        suffix: "7",
        intervals: &[0, 4, 7, 10],
    },
    Quality {
        name: "Major 7",
        suffix: "maj7",
        intervals: &[0, 4, 7, 11],
    },
    Quality {
        name: "Minor 7",
        suffix: "m7",
        intervals: &[0, 3, 7, 10],
    },
    Quality {
        name: "Diminished",
        suffix: "dim",
        intervals: &[0, 3, 6],
    },
    Quality {
        name: "Dim 7",
        suffix: "dim7",
        intervals: &[0, 3, 6, 9],
    },
    Quality {
        name: "Half-dim",
        suffix: "m7b5",
        intervals: &[0, 3, 6, 10],
    },
    Quality {
        name: "Augmented",
        suffix: "aug",
        intervals: &[0, 4, 8],
    },
    Quality {
        name: "Sus 2",
        suffix: "sus2",
        intervals: &[0, 2, 7],
    },
    Quality {
        name: "Sus 4",
        suffix: "sus4",
        intervals: &[0, 5, 7],
    },
    Quality {
        name: "6",
        suffix: "6",
        intervals: &[0, 4, 7, 9],
    },
    Quality {
        name: "Minor 6",
        suffix: "m6",
        intervals: &[0, 3, 7, 9],
    },
    Quality {
        name: "Add 9",
        suffix: "add9",
        intervals: &[0, 4, 7, 14],
    },
    Quality {
        name: "9",
        suffix: "9",
        intervals: &[0, 4, 7, 10, 14],
    },
    Quality {
        name: "Minor 9",
        suffix: "m9",
        intervals: &[0, 3, 7, 10, 14],
    },
    Quality {
        name: "Major 9",
        suffix: "maj9",
        intervals: &[0, 4, 7, 11, 14],
    },
    Quality {
        name: "Power (5)",
        suffix: "5",
        intervals: &[0, 7],
    },
];

/// Extra spellings accepted when parsing, mapped to a canonical suffix.
const ALIASES: &[(&str, &str)] = &[
    ("maj", ""),
    ("M", ""),
    ("min", "m"),
    ("-", "m"),
    ("M7", "maj7"),
    ("Δ", "maj7"),
    ("Δ7", "maj7"),
    ("min7", "m7"),
    ("-7", "m7"),
    ("o", "dim"),
    ("°", "dim"),
    ("o7", "dim7"),
    ("°7", "dim7"),
    ("ø", "m7b5"),
    ("ø7", "m7b5"),
    ("+", "aug"),
    ("sus", "sus4"),
    ("dom7", "7"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chord {
    /// Pitch class of the root, 0 = C.
    pub root: u8,
    pub quality: usize,
    /// 0 = root position, 1 = first inversion, …
    pub inversion: usize,
    /// Optional bass note (pitch class) for slash chords like C/G.
    pub bass: Option<u8>,
}

impl Chord {
    pub fn new(root: u8, quality: usize) -> Self {
        Chord {
            root: root % 12,
            quality,
            inversion: 0,
            bass: None,
        }
    }

    pub fn q(&self) -> &'static Quality {
        &QUALITIES[self.quality.min(QUALITIES.len() - 1)]
    }

    /// e.g. "F#m7/C#"
    pub fn symbol(&self) -> String {
        let mut s = format!("{}{}", NOTE_NAMES[self.root as usize], self.q().suffix);
        if let Some(b) = self.bass {
            s.push('/');
            s.push_str(NOTE_NAMES[b as usize]);
        }
        s
    }

    /// e.g. "F# Minor 7, 1st inversion"
    pub fn long_name(&self) -> String {
        let mut s = format!("{} {}", NOTE_NAMES[self.root as usize], self.q().name);
        if self.inversion > 0 {
            s.push_str(&format!(", {} inversion", ordinal(self.inversion)));
        }
        if let Some(b) = self.bass {
            s.push_str(&format!(", {} in the bass", NOTE_NAMES[b as usize]));
        }
        s
    }

    /// MIDI notes to light, lowest first. `base` is the C the chord is built from
    /// (60 = middle C). The result is shifted by octaves to fit inside `range`.
    pub fn notes(&self, base: u8, range: (u8, u8)) -> Vec<u8> {
        let iv = self.q().intervals;
        let inv = self.inversion % iv.len();
        let root = base as i32 + self.root as i32;
        let mut notes: Vec<i32> = iv.iter().map(|&i| root + i as i32).collect();
        // Inversion: move the lowest notes up an octave.
        for n in notes.iter_mut().take(inv) {
            *n += 12;
        }
        notes.sort_unstable();
        if let Some(b) = self.bass {
            // Slash bass goes below the chord.
            let mut bass = base as i32 + b as i32;
            while bass >= notes[0] {
                bass -= 12;
            }
            notes.insert(0, bass);
        }
        let (lo, hi) = (range.0 as i32, range.1 as i32);
        while notes.last().is_some_and(|&n| n > hi) && notes[0] - 12 >= lo {
            notes.iter_mut().for_each(|n| *n -= 12);
        }
        while notes[0] < lo && notes.last().is_some_and(|&n| n + 12 <= hi) {
            notes.iter_mut().for_each(|n| *n += 12);
        }
        notes
            .into_iter()
            .filter(|&n| (lo..=hi).contains(&n))
            .map(|n| n as u8)
            .collect()
    }

    /// The MIDI note that is the chord's root in `notes()`.
    pub fn is_root(&self, note: u8) -> bool {
        note % 12 == self.root
    }
}

fn ordinal(n: usize) -> &'static str {
    match n {
        1 => "1st",
        2 => "2nd",
        3 => "3rd",
        _ => "4th",
    }
}

fn parse_note(s: &str) -> Option<(u8, &str)> {
    let mut chars = s.chars();
    let letter = chars.next()?.to_ascii_uppercase();
    let base: i32 = match letter {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let rest = &s[1..];
    let (shift, rest) = if let Some(r) = rest.strip_prefix('#').or_else(|| rest.strip_prefix('♯'))
    {
        (1, r)
    } else if let Some(r) = rest.strip_prefix('b').or_else(|| rest.strip_prefix('♭')) {
        (-1, r)
    } else {
        (0, rest)
    };
    Some(((base + shift).rem_euclid(12) as u8, rest))
}

/// Parse one chord symbol, e.g. "C", "Am", "F#m7", "Bbmaj7", "G7/B", "Dsus4".
pub fn parse(symbol: &str) -> Option<Chord> {
    let symbol = symbol.trim();
    let (main, bass) = match symbol.split_once('/') {
        Some((m, b)) => (m, Some(parse_note(b).filter(|(_, r)| r.is_empty())?.0)),
        None => (symbol, None),
    };
    let (root, suffix) = parse_note(main)?;
    let suffix = ALIASES
        .iter()
        .find(|(a, _)| *a == suffix)
        .map_or(suffix, |(_, c)| *c);
    let quality = QUALITIES.iter().position(|q| q.suffix == suffix)?;
    Some(Chord {
        root,
        quality,
        inversion: 0,
        bass,
    })
}

/// Parse a progression like "C G Am F" or "C | G | Am | F". Returns the chords and any
/// symbols that couldn't be read.
pub fn parse_progression(text: &str) -> (Vec<Chord>, Vec<String>) {
    let mut chords = Vec::new();
    let mut bad = Vec::new();
    for tok in text
        .split(|c: char| c.is_whitespace() || c == ',' || c == '|')
        .filter(|t| !t.is_empty())
    {
        match parse(tok) {
            Some(c) => chords.push(c),
            None => bad.push(tok.to_string()),
        }
    }
    (chords, bad)
}

/// Name the chord formed by the notes being held, e.g. [64, 67, 72] → C/E.
/// Returns None for single notes and note clusters that aren't a known chord.
pub fn identify(notes: &[u8]) -> Option<Chord> {
    let bass = *notes.iter().min()? % 12;
    let mut pcs: Vec<u8> = notes.iter().map(|n| n % 12).collect();
    pcs.sort_unstable();
    pcs.dedup();
    if pcs.len() < 2 {
        return None;
    }
    let set = |v: &[u8]| -> u16 { v.iter().fold(0u16, |m, &p| m | 1 << (p % 12)) };
    let held = set(&pcs);

    // (how good the match is, root isn't the bass, quality order) → lower is better
    let mut best: Option<((u8, bool, usize), Chord)> = None;
    for &root in &pcs {
        let rel = |m: u16| -> u16 { ((m >> root) | (m << (12 - root))) & 0xFFF };
        let held_rel = rel(held);
        let bass_rel = 1u16 << ((bass + 12 - root) % 12);
        for (qi, q) in QUALITIES.iter().enumerate() {
            let shape = set(q.intervals);
            if q.suffix == "5" && pcs.len() != 2 {
                continue; // power chord only for bare fifths
            }
            let score = if held_rel == shape {
                0
            } else if q.intervals.len() >= 4 && shape & 1 << 7 != 0 && held_rel == shape & !(1 << 7)
            {
                1 // seventh/ninth chord with the fifth left out
            } else if bass_rel & shape == 0 && held_rel & !bass_rel == shape {
                2 // chord over a different bass note
            } else {
                continue;
            };
            let key = (score, root != bass, qi);
            if best.as_ref().is_none_or(|(k, _)| key < *k) {
                let chord = Chord {
                    root,
                    quality: qi,
                    inversion: 0,
                    bass: (root != bass).then_some(bass),
                };
                best = Some((key, chord));
            }
        }
    }
    best.map(|(_, c)| c)
}

/// Name of the interval between two notes, for when only two keys are held.
pub fn interval_name(a: u8, b: u8) -> &'static str {
    const NAMES: [&str; 12] = [
        "Octave",
        "Minor 2nd",
        "Major 2nd",
        "Minor 3rd",
        "Major 3rd",
        "Perfect 4th",
        "Tritone",
        "Perfect 5th",
        "Minor 6th",
        "Major 6th",
        "Minor 7th",
        "Major 7th",
    ];
    NAMES[(a.abs_diff(b) % 12) as usize]
}

pub fn note_name(n: u8) -> String {
    format!("{}{}", NOTE_NAMES[(n % 12) as usize], n / 12 - 1)
}

/// A scale: name and semitones above the root within one octave.
pub struct Scale {
    pub name: &'static str,
    pub intervals: &'static [u8],
}

pub const SCALES: &[Scale] = &[
    Scale {
        name: "Major",
        intervals: &[0, 2, 4, 5, 7, 9, 11],
    },
    Scale {
        name: "Natural minor",
        intervals: &[0, 2, 3, 5, 7, 8, 10],
    },
    Scale {
        name: "Harmonic minor",
        intervals: &[0, 2, 3, 5, 7, 8, 11],
    },
    Scale {
        name: "Melodic minor",
        intervals: &[0, 2, 3, 5, 7, 9, 11],
    },
    Scale {
        name: "Major pentatonic",
        intervals: &[0, 2, 4, 7, 9],
    },
    Scale {
        name: "Minor pentatonic",
        intervals: &[0, 3, 5, 7, 10],
    },
    Scale {
        name: "Blues",
        intervals: &[0, 3, 5, 6, 7, 10],
    },
    Scale {
        name: "Dorian",
        intervals: &[0, 2, 3, 5, 7, 9, 10],
    },
    Scale {
        name: "Phrygian",
        intervals: &[0, 1, 3, 5, 7, 8, 10],
    },
    Scale {
        name: "Lydian",
        intervals: &[0, 2, 4, 6, 7, 9, 11],
    },
    Scale {
        name: "Mixolydian",
        intervals: &[0, 2, 4, 5, 7, 9, 10],
    },
    Scale {
        name: "Locrian",
        intervals: &[0, 1, 3, 5, 6, 8, 10],
    },
    Scale {
        name: "Whole tone",
        intervals: &[0, 2, 4, 6, 8, 10],
    },
    Scale {
        name: "Chromatic",
        intervals: &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11],
    },
];

/// Keys of a scale. With `whole_keyboard`, every key in `range` that's in the scale;
/// otherwise one octave going up from the root at `base` (including the top root).
pub fn scale_notes(
    root: u8,
    scale: &Scale,
    base: u8,
    range: (u8, u8),
    whole_keyboard: bool,
) -> Vec<u8> {
    let (lo, hi) = range;
    if whole_keyboard {
        return (lo..=hi)
            .filter(|n| scale.intervals.contains(&((n + 12 - root % 12) % 12)))
            .collect();
    }
    let start = base + root % 12;
    scale
        .intervals
        .iter()
        .map(|i| start + i)
        .chain(std::iter::once(start + 12))
        .filter(|n| (lo..=hi).contains(n))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    const KEYS61: (u8, u8) = (36, 96);

    #[test]
    fn c_major_at_middle_c() {
        assert_eq!(parse("C").unwrap().notes(60, KEYS61), vec![60, 64, 67]);
    }

    #[test]
    fn inversions() {
        let mut c = parse("C").unwrap();
        c.inversion = 1;
        assert_eq!(c.notes(60, KEYS61), vec![64, 67, 72]);
        c.inversion = 2;
        assert_eq!(c.notes(60, KEYS61), vec![67, 72, 76]);
    }

    #[test]
    fn parses_symbols() {
        assert_eq!(parse("F#m7").unwrap().symbol(), "F#m7");
        assert_eq!(
            parse("Bbmaj7").unwrap().notes(60, KEYS61),
            vec![70, 74, 77, 81]
        );
        assert_eq!(parse("Am").unwrap().notes(60, KEYS61), vec![69, 72, 76]);
        assert_eq!(parse("Gsus").unwrap().symbol(), "Gsus4");
        assert_eq!(parse("C-7").unwrap().symbol(), "Cm7");
        assert!(parse("H7").is_none());
        assert!(parse("Cxyz").is_none());
    }

    #[test]
    fn slash_bass_goes_below() {
        assert_eq!(
            parse("C/G").unwrap().notes(60, KEYS61),
            vec![55, 60, 64, 67]
        );
        assert_eq!(
            parse("D/F#").unwrap().notes(60, KEYS61),
            vec![54, 62, 66, 69]
        );
    }

    #[test]
    fn stays_on_the_keyboard() {
        // B major 9 from a high base would run off the top of a 61-key keyboard.
        let notes = parse("B9").unwrap().notes(84, KEYS61);
        assert!(notes.iter().all(|&n| (36..=96).contains(&n)));
        assert_eq!(notes.len(), 5);
    }

    #[test]
    fn progression() {
        let (chords, bad) = parse_progression("C G | Am, F  Zz");
        assert_eq!(
            chords.iter().map(Chord::symbol).collect::<Vec<_>>(),
            ["C", "G", "Am", "F"]
        );
        assert_eq!(bad, ["Zz"]);
    }

    fn name(notes: &[u8]) -> Option<String> {
        identify(notes).map(|c| c.symbol())
    }

    #[test]
    fn identifies_chords() {
        assert_eq!(name(&[60, 64, 67]).as_deref(), Some("C"));
        assert_eq!(name(&[64, 67, 72]).as_deref(), Some("C/E"));
        assert_eq!(name(&[55, 60, 64]).as_deref(), Some("C/G"));
        assert_eq!(name(&[57, 60, 64]).as_deref(), Some("Am"));
        assert_eq!(name(&[60, 64, 67, 70]).as_deref(), Some("C7"));
        assert_eq!(name(&[60, 64, 70]).as_deref(), Some("C7")); // no fifth
        assert_eq!(name(&[62, 65, 69, 72]).as_deref(), Some("Dm7"));
        assert_eq!(name(&[60, 67]).as_deref(), Some("C5"));
        assert_eq!(name(&[48, 60, 64, 67, 76]).as_deref(), Some("C")); // doubled notes
        // Same notes, bass decides: A C E G with A at the bottom is Am7, not C6/A.
        assert_eq!(name(&[57, 60, 64, 67]).as_deref(), Some("Am7"));
        assert_eq!(name(&[60, 64, 67, 69]).as_deref(), Some("C6"));
    }

    #[test]
    fn not_chords() {
        assert_eq!(name(&[60]), None);
        assert_eq!(name(&[60, 72]), None);
        assert_eq!(name(&[60, 61, 62]), None);
        assert_eq!(name(&[]), None);
    }

    #[test]
    fn scales() {
        let major = &SCALES[0];
        assert_eq!(
            scale_notes(0, major, 60, KEYS61, false),
            vec![60, 62, 64, 65, 67, 69, 71, 72]
        );
        // A minor pentatonic across a 61-key keyboard: 5 notes per octave, all in range.
        let pent = scale_notes(9, &SCALES[5], 60, KEYS61, true);
        assert!(pent.iter().all(|n| [9, 0, 2, 4, 7].contains(&(n % 12))));
        assert_eq!(pent.len(), 26);
    }
}
