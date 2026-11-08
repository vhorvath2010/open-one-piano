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
}
