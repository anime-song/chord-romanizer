//! Structured chord-quality syntax.
//!
//! The original Python implementation kept the suffix as one string and used
//! substring checks such as `contains("m")`. That is convenient for display
//! but unsafe for analysis: an altered tension can accidentally change the
//! inferred triad. This module keeps the raw suffix while also parsing the
//! semantic pieces needed to build one authoritative chord formula.

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// Base triad or suspended/power structure.
pub enum QualityClass {
    Major,
    Minor,
    Diminished,
    HalfDiminished,
    Augmented,
    Suspended2,
    Suspended4,
    Power,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// Quality of the seventh independently of the base triad.
pub enum SeventhQuality {
    Major,
    Minor,
    Diminished,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// Chord-member number rather than a scale degree relative to the song key.
pub enum ChordDegree {
    Root,
    Second,
    Third,
    Fourth,
    Fifth,
    Sixth,
    Seventh,
    Ninth,
    Eleventh,
    Thirteenth,
}

impl ChordDegree {
    pub const fn number(self) -> u8 {
        match self {
            Self::Root => 1,
            Self::Second => 2,
            Self::Third => 3,
            Self::Fourth => 4,
            Self::Fifth => 5,
            Self::Sixth => 6,
            Self::Seventh => 7,
            Self::Ninth => 9,
            Self::Eleventh => 11,
            Self::Thirteenth => 13,
        }
    }

    pub fn parse(number: u8) -> Option<Self> {
        match number {
            1 => Some(Self::Root),
            2 => Some(Self::Second),
            3 => Some(Self::Third),
            4 => Some(Self::Fourth),
            5 => Some(Self::Fifth),
            6 => Some(Self::Sixth),
            7 => Some(Self::Seventh),
            9 => Some(Self::Ninth),
            11 => Some(Self::Eleventh),
            13 => Some(Self::Thirteenth),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// How a degree entered the chord formula.
pub enum ModifierKind {
    /// Implied by an extension symbol such as the natural ninth in `C9`.
    Implied,
    /// Explicitly added without implying the lower extension stack (`Cadd9`).
    Added,
    /// Explicitly chromatically altered (`b9`, `#11`).
    Altered,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DegreeModifier {
    pub degree: ChordDegree,
    pub alteration: i8,
    pub kind: ModifierKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Parsed quality plus the untouched suffix needed for faithful rendering.
pub struct ChordQuality {
    pub raw: String,
    pub class: QualityClass,
    pub seventh: Option<SeventhQuality>,
    pub modifiers: Vec<DegreeModifier>,
    pub omissions: Vec<ChordDegree>,
    pub unknown_tokens: Vec<String>,
}

impl ChordQuality {
    /// Parse a chord suffix without rejecting unknown notation.
    ///
    /// Unknown fragments are retained in `unknown_tokens`. Formula generation
    /// may then decline to guess, while a formatter can still preserve exactly
    /// what the user wrote.
    pub fn parse(raw: &str) -> Self {
        let compact = raw.replace(' ', "");
        // Some MIDI chord-marker formats separate the root and quality with a
        // colon (`B:7/D#`, `C#:m7`). The colon is presentation syntax rather
        // than part of the quality, so ignore one leading separator for
        // analysis while retaining `raw` for lossless rendering.
        let semantic = compact.strip_prefix(':').unwrap_or(&compact);
        let (class, seventh, implied_extensions, remainder) = parse_base(semantic);
        let mut parsed = Self {
            raw: raw.to_owned(),
            class,
            seventh,
            modifiers: implied_extensions,
            omissions: Vec::new(),
            unknown_tokens: Vec::new(),
        };
        parse_modifiers(remainder, &mut parsed);
        parsed
    }

    pub fn is_fully_recognized(&self) -> bool {
        self.class != QualityClass::Unknown && self.unknown_tokens.is_empty()
    }
}

type ParsedBase<'a> = (
    QualityClass,
    Option<SeventhQuality>,
    Vec<DegreeModifier>,
    &'a str,
);

fn parse_base(compact: &str) -> ParsedBase<'_> {
    let implied = |degrees: &[ChordDegree]| {
        degrees
            .iter()
            .copied()
            .map(|degree| DegreeModifier {
                degree,
                alteration: 0,
                kind: ModifierKind::Implied,
            })
            .collect()
    };

    // Longest/specific spellings must precede short prefixes: `mMaj9` must be
    // recognized before `m`, and `maj7` before a bare major default. This table
    // is deliberately explicit so supported notation is auditable.
    let bases: &[(&str, QualityClass, Option<SeventhQuality>, &[ChordDegree])] = &[
        (
            "minMaj13",
            QualityClass::Minor,
            Some(SeventhQuality::Major),
            &[
                ChordDegree::Ninth,
                ChordDegree::Eleventh,
                ChordDegree::Thirteenth,
            ],
        ),
        (
            "minMaj11",
            QualityClass::Minor,
            Some(SeventhQuality::Major),
            &[ChordDegree::Ninth, ChordDegree::Eleventh],
        ),
        (
            "minMaj9",
            QualityClass::Minor,
            Some(SeventhQuality::Major),
            &[ChordDegree::Ninth],
        ),
        (
            "minMaj7",
            QualityClass::Minor,
            Some(SeventhQuality::Major),
            &[],
        ),
        (
            "mMaj13",
            QualityClass::Minor,
            Some(SeventhQuality::Major),
            &[
                ChordDegree::Ninth,
                ChordDegree::Eleventh,
                ChordDegree::Thirteenth,
            ],
        ),
        (
            "mMaj11",
            QualityClass::Minor,
            Some(SeventhQuality::Major),
            &[ChordDegree::Ninth, ChordDegree::Eleventh],
        ),
        (
            "mMaj9",
            QualityClass::Minor,
            Some(SeventhQuality::Major),
            &[ChordDegree::Ninth],
        ),
        (
            "mMaj7",
            QualityClass::Minor,
            Some(SeventhQuality::Major),
            &[],
        ),
        ("mM7", QualityClass::Minor, Some(SeventhQuality::Major), &[]),
        (
            "m7-5",
            QualityClass::HalfDiminished,
            Some(SeventhQuality::Minor),
            &[],
        ),
        (
            "m7b5",
            QualityClass::HalfDiminished,
            Some(SeventhQuality::Minor),
            &[],
        ),
        (
            "maj13",
            QualityClass::Major,
            Some(SeventhQuality::Major),
            &[
                ChordDegree::Ninth,
                ChordDegree::Eleventh,
                ChordDegree::Thirteenth,
            ],
        ),
        (
            "maj11",
            QualityClass::Major,
            Some(SeventhQuality::Major),
            &[ChordDegree::Ninth, ChordDegree::Eleventh],
        ),
        (
            "maj9",
            QualityClass::Major,
            Some(SeventhQuality::Major),
            &[ChordDegree::Ninth],
        ),
        (
            "maj7",
            QualityClass::Major,
            Some(SeventhQuality::Major),
            &[],
        ),
        (
            "ma13",
            QualityClass::Major,
            Some(SeventhQuality::Major),
            &[
                ChordDegree::Ninth,
                ChordDegree::Eleventh,
                ChordDegree::Thirteenth,
            ],
        ),
        (
            "ma11",
            QualityClass::Major,
            Some(SeventhQuality::Major),
            &[ChordDegree::Ninth, ChordDegree::Eleventh],
        ),
        (
            "ma9",
            QualityClass::Major,
            Some(SeventhQuality::Major),
            &[ChordDegree::Ninth],
        ),
        ("ma7", QualityClass::Major, Some(SeventhQuality::Major), &[]),
        (
            "M13",
            QualityClass::Major,
            Some(SeventhQuality::Major),
            &[
                ChordDegree::Ninth,
                ChordDegree::Eleventh,
                ChordDegree::Thirteenth,
            ],
        ),
        (
            "M11",
            QualityClass::Major,
            Some(SeventhQuality::Major),
            &[ChordDegree::Ninth, ChordDegree::Eleventh],
        ),
        (
            "M9",
            QualityClass::Major,
            Some(SeventhQuality::Major),
            &[ChordDegree::Ninth],
        ),
        ("M7", QualityClass::Major, Some(SeventhQuality::Major), &[]),
        (
            "dim7",
            QualityClass::Diminished,
            Some(SeventhQuality::Diminished),
            &[],
        ),
        (
            "o7",
            QualityClass::Diminished,
            Some(SeventhQuality::Diminished),
            &[],
        ),
        (
            "ø7",
            QualityClass::HalfDiminished,
            Some(SeventhQuality::Minor),
            &[],
        ),
        ("sus2", QualityClass::Suspended2, None, &[]),
        ("sus4", QualityClass::Suspended4, None, &[]),
        ("aug", QualityClass::Augmented, None, &[]),
        ("dim", QualityClass::Diminished, None, &[]),
        (
            "min13",
            QualityClass::Minor,
            Some(SeventhQuality::Minor),
            &[
                ChordDegree::Ninth,
                ChordDegree::Eleventh,
                ChordDegree::Thirteenth,
            ],
        ),
        (
            "min11",
            QualityClass::Minor,
            Some(SeventhQuality::Minor),
            &[ChordDegree::Ninth, ChordDegree::Eleventh],
        ),
        (
            "min9",
            QualityClass::Minor,
            Some(SeventhQuality::Minor),
            &[ChordDegree::Ninth],
        ),
        (
            "min7",
            QualityClass::Minor,
            Some(SeventhQuality::Minor),
            &[],
        ),
        ("min6", QualityClass::Minor, None, &[ChordDegree::Sixth]),
        ("min", QualityClass::Minor, None, &[]),
        (
            "m13",
            QualityClass::Minor,
            Some(SeventhQuality::Minor),
            &[
                ChordDegree::Ninth,
                ChordDegree::Eleventh,
                ChordDegree::Thirteenth,
            ],
        ),
        (
            "m11",
            QualityClass::Minor,
            Some(SeventhQuality::Minor),
            &[ChordDegree::Ninth, ChordDegree::Eleventh],
        ),
        (
            "m9",
            QualityClass::Minor,
            Some(SeventhQuality::Minor),
            &[ChordDegree::Ninth],
        ),
        ("m7", QualityClass::Minor, Some(SeventhQuality::Minor), &[]),
        ("m6", QualityClass::Minor, None, &[ChordDegree::Sixth]),
        ("m", QualityClass::Minor, None, &[]),
        (
            "13",
            QualityClass::Major,
            Some(SeventhQuality::Minor),
            &[
                ChordDegree::Ninth,
                ChordDegree::Eleventh,
                ChordDegree::Thirteenth,
            ],
        ),
        (
            "11",
            QualityClass::Major,
            Some(SeventhQuality::Minor),
            &[ChordDegree::Ninth, ChordDegree::Eleventh],
        ),
        (
            "9",
            QualityClass::Major,
            Some(SeventhQuality::Minor),
            &[ChordDegree::Ninth],
        ),
        ("7", QualityClass::Major, Some(SeventhQuality::Minor), &[]),
        ("6", QualityClass::Major, None, &[ChordDegree::Sixth]),
        ("5", QualityClass::Power, None, &[]),
        ("+", QualityClass::Augmented, None, &[]),
        ("o", QualityClass::Diminished, None, &[]),
    ];

    if compact.is_empty() {
        return (QualityClass::Major, None, Vec::new(), "");
    }
    for (prefix, class, seventh, extensions) in bases {
        if let Some(remainder) = compact.strip_prefix(prefix) {
            return (*class, *seventh, implied(extensions), remainder);
        }
    }
    // A suffix may consist only of modifiers (`Cadd9`, `C(b5)`). Such forms use
    // a major base unless another explicit base token says otherwise.
    if compact.starts_with("add")
        || compact.starts_with("omit")
        || compact.starts_with("no")
        || compact.starts_with(['#', 'b', '('])
    {
        return (QualityClass::Major, None, Vec::new(), compact);
    }
    (QualityClass::Unknown, None, Vec::new(), compact)
}

fn parse_modifiers(remainder: &str, quality: &mut ChordQuality) {
    // Parentheses and commas are presentation choices, not semantic grouping
    // for the supported grammar. Normalize both to a simple token stream.
    let normalized = remainder.replace(['(', ')'], ",");
    let mut cursor = normalized.as_str();
    while !cursor.is_empty() {
        cursor = cursor.trim_start_matches(',');
        if cursor.is_empty() {
            break;
        }

        let end = cursor.find(',').unwrap_or(cursor.len());
        let segment = &cursor[..end];
        cursor = &cursor[end..];

        // A segment may contain chained modifiers such as "add9omit3". Consume
        // from the left so every recognized token records its exact role.
        let mut segment_cursor = segment;
        while !segment_cursor.is_empty() {
            if let Some(rest) = segment_cursor.strip_prefix("sus2") {
                quality.class = QualityClass::Suspended2;
                segment_cursor = rest;
                continue;
            }
            if let Some(rest) = segment_cursor.strip_prefix("sus4") {
                quality.class = QualityClass::Suspended4;
                segment_cursor = rest;
                continue;
            }
            if let Some((degree, consumed)) = prefixed_degree(segment_cursor, &["omit", "no"]) {
                quality.omissions.push(degree);
                segment_cursor = &segment_cursor[consumed..];
                continue;
            }
            if let Some((degree, alteration, consumed)) = altered_degree(segment_cursor, "add") {
                quality.modifiers.push(DegreeModifier {
                    degree,
                    alteration,
                    kind: ModifierKind::Added,
                });
                segment_cursor = &segment_cursor[consumed..];
                continue;
            }
            if let Some((degree, alteration, consumed)) = altered_degree(segment_cursor, "") {
                // An unaltered bare degree in a modifier position is an added
                // tone. Only extension bases (`9`, `11`, `13`) create implied
                // lower extension stacks.
                quality.modifiers.push(DegreeModifier {
                    degree,
                    alteration,
                    kind: if alteration == 0 {
                        ModifierKind::Added
                    } else {
                        ModifierKind::Altered
                    },
                });
                segment_cursor = &segment_cursor[consumed..];
                continue;
            }
            if let Some(rest) = segment_cursor.strip_prefix('7') {
                quality.seventh = Some(SeventhQuality::Minor);
                segment_cursor = rest;
                continue;
            }
            // Stop at the first unknown remainder. Skipping characters in hope
            // of finding a later known token could make an invalid formula look
            // trustworthy.
            quality.unknown_tokens.push(segment_cursor.to_owned());
            break;
        }
    }
}

fn prefixed_degree(text: &str, prefixes: &[&str]) -> Option<(ChordDegree, usize)> {
    for prefix in prefixes {
        if let Some(rest) = text.strip_prefix(prefix) {
            let (degree, length) = parse_degree_number(rest)?;
            return Some((degree, prefix.len() + length));
        }
    }
    None
}

fn altered_degree(text: &str, prefix: &str) -> Option<(ChordDegree, i8, usize)> {
    let mut rest = text.strip_prefix(prefix)?;
    let mut consumed = prefix.len();
    let mut alteration = 0_i8;
    while let Some(first) = rest.chars().next() {
        match first {
            '#' => alteration += 1,
            // `7-5` is a common flat-five spelling in MIDI chord markers.
            // Treat the minus sign as an accidental only in this modifier
            // position; the untouched raw quality remains `7-5`.
            'b' | '-' => alteration -= 1,
            _ => break,
        }
        consumed += first.len_utf8();
        rest = &rest[first.len_utf8()..];
    }
    let (degree, length) = parse_degree_number(rest)?;
    Some((degree, alteration, consumed + length))
}

fn parse_degree_number(text: &str) -> Option<(ChordDegree, usize)> {
    for number in [13_u8, 11, 9, 7, 6, 5, 4, 3, 2, 1] {
        let token = number.to_string();
        if text.starts_with(&token) {
            return Some((ChordDegree::parse(number)?, token.len()));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_altered_tensions_without_inventing_natural_nine() {
        let quality = ChordQuality::parse("7(b9,#11)");
        assert_eq!(quality.class, QualityClass::Major);
        assert_eq!(quality.seventh, Some(SeventhQuality::Minor));
        assert_eq!(quality.modifiers.len(), 2);
        assert_eq!(quality.modifiers[0].degree, ChordDegree::Ninth);
        assert_eq!(quality.modifiers[0].alteration, -1);
        assert_eq!(quality.modifiers[1].degree, ChordDegree::Eleventh);
        assert_eq!(quality.modifiers[1].alteration, 1);
        assert!(quality.is_fully_recognized());
    }

    #[test]
    fn distinguishes_add_nine_from_extended_nine() {
        let add_nine = ChordQuality::parse("add9");
        assert_eq!(add_nine.seventh, None);
        assert_eq!(add_nine.modifiers[0].kind, ModifierKind::Added);

        let dominant_nine = ChordQuality::parse("9");
        assert_eq!(dominant_nine.seventh, Some(SeventhQuality::Minor));
        assert_eq!(dominant_nine.modifiers[0].kind, ModifierKind::Implied);
    }

    #[test]
    fn parses_suspension_and_omission() {
        let quality = ChordQuality::parse("7sus4(omit5)");
        assert_eq!(quality.class, QualityClass::Suspended4);
        assert_eq!(quality.omissions, [ChordDegree::Fifth]);
        assert!(quality.is_fully_recognized());
    }

    #[test]
    fn parses_midi_marker_quality_spellings_without_losing_raw_text() {
        for raw in [
            ":m7", ":aug", ":M7", ":m7-5", ":7", ":m7(9)", ":m", ":dim7", ":sus4", ":dim",
            ":7(b9)", ":7(b13)", ":7(13)", ":M7(9)", ":7-5", ":6", ":7sus4", ":7(9,13)", ":m7(11)",
            ":7(9)", ":mM7",
        ] {
            assert!(
                ChordQuality::parse(raw).is_fully_recognized(),
                "MIDI marker quality should be recognized: {raw}"
            );
        }

        let dominant = ChordQuality::parse(":7");
        assert_eq!(dominant.raw, ":7");
        assert_eq!(dominant.class, QualityClass::Major);
        assert_eq!(dominant.seventh, Some(SeventhQuality::Minor));
        assert!(dominant.is_fully_recognized());

        let flat_five = ChordQuality::parse(":7-5");
        assert_eq!(flat_five.raw, ":7-5");
        assert!(flat_five.modifiers.iter().any(|modifier| {
            modifier.degree == ChordDegree::Fifth && modifier.alteration == -1
        }));
        assert!(flat_five.is_fully_recognized());

        let minor_major = ChordQuality::parse(":mM7");
        assert_eq!(minor_major.raw, ":mM7");
        assert_eq!(minor_major.class, QualityClass::Minor);
        assert_eq!(minor_major.seventh, Some(SeventhQuality::Major));
        assert!(minor_major.is_fully_recognized());
    }
}
