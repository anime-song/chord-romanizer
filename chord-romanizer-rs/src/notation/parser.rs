//! Chord-symbol parser.
//!
//! Parsing is intentionally lossless at the notation boundary: root, quality,
//! slash bass, and original lexemes are retained. Semantic validation of the
//! quality happens later, allowing unknown suffixes to round-trip without
//! pretending they describe a known major chord.

use crate::domain::{ChordQuality, ParsedChord, ParsedSymbol, SpelledNote};
use crate::error::ParseError;

/// Parse a chord, N.C. marker, or other supported progression symbol.
///
/// The standalone parser does not create boundaries; callers add those with
/// `ProgressionItem::boundary` because a long silence cannot be inferred from
/// the text `N.C.` alone.
pub fn parse_chord(symbol: &str) -> Result<ParsedSymbol, ParseError> {
    let text = symbol.trim();
    if text.is_empty() {
        return Err(ParseError::EmptyInput);
    }

    // Normalize only for recognition. Keep the trimmed original spelling in
    // the AST so an aligned editor can reproduce the user's input.
    let normalized_nc = text.replace(['.', ' '], "").to_ascii_uppercase();
    if matches!(normalized_nc.as_str(), "NC" | "NOCHORD") {
        return Ok(ParsedSymbol::NoChord {
            original_symbol: text.to_owned(),
        });
    }

    // The first slash is the structural separator. A second slash remains in
    // `bass` and fails note parsing rather than being silently discarded.
    let (body, bass_token) = if let Some((body, bass)) = text.split_once('/') {
        let bass = bass.trim();
        let parsed =
            SpelledNote::parse(bass).map_err(|_| ParseError::InvalidBass(bass.to_owned()))?;
        (body, Some((bass, parsed)))
    } else {
        (text, None)
    };

    let body = body.trim();
    if body.is_empty() {
        return Err(ParseError::InvalidRoot(body.to_owned()));
    }

    // A root token is one note letter followed by contiguous accidentals.
    // Quality parsing starts at the first non-accidental character.
    let mut root_end = body
        .char_indices()
        .next()
        .map(|(index, ch)| index + ch.len_utf8())
        .ok_or_else(|| ParseError::InvalidRoot(body.to_owned()))?;
    for (index, ch) in body[root_end..].char_indices() {
        if matches!(ch, '#' | 'b' | 'B' | 'x' | 'X') {
            root_end += ch.len_utf8();
        } else {
            let _ = index;
            break;
        }
    }

    let root_token = &body[..root_end];
    let root = SpelledNote::parse(root_token)
        .map_err(|_| ParseError::InvalidRoot(root_token.to_owned()))?;
    let root_lexeme = normalize_spelling(root_token);
    // Keep raw quality for exact rendering and parse a second structured view
    // for theory. Neither representation has to be reconstructed from the
    // other later.
    let quality = body[root_end..].to_owned();
    let quality_parsed = ChordQuality::parse(&quality);
    let (bass, bass_lexeme) = bass_token
        .map(|(raw, note)| (Some(note), Some(normalize_spelling(raw))))
        .unwrap_or((None, None));

    Ok(ParsedSymbol::Chord(ParsedChord {
        original_symbol: text.to_owned(),
        root,
        root_lexeme,
        quality,
        quality_parsed,
        bass,
        bass_lexeme,
    }))
}

fn normalize_spelling(token: &str) -> String {
    let mut chars = token.chars();
    match chars.next() {
        Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(symbol: &str) -> ParsedChord {
        match parse_chord(symbol).unwrap() {
            ParsedSymbol::Chord(chord) => chord,
            ParsedSymbol::NoChord { .. } | ParsedSymbol::Boundary { .. } => {
                panic!("expected a chord")
            }
        }
    }

    #[test]
    fn parses_root_quality_and_bass() {
        let parsed = chord("C#m7/G#");
        assert_eq!(parsed.root.to_string(), "C#");
        assert_eq!(parsed.quality, "m7");
        assert_eq!(parsed.bass.unwrap().to_string(), "G#");
    }

    #[test]
    fn preserves_double_accidental_lexemes() {
        assert_eq!(chord("Bbb7").root_lexeme, "Bbb");
        assert_eq!(chord("F##m7").root_lexeme, "F##");
        assert_eq!(chord("C7/Bbb").bass_lexeme.as_deref(), Some("Bbb"));
    }

    #[test]
    fn recognizes_no_chord() {
        assert!(matches!(
            parse_chord("N.C.").unwrap(),
            ParsedSymbol::NoChord { .. }
        ));
    }

    #[test]
    fn rejects_invalid_bass() {
        assert!(matches!(
            parse_chord("C/G/Db"),
            Err(ParseError::InvalidBass(_))
        ));
    }
}
