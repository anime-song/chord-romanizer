use crate::domain::{Degree, SpelledNote};

pub fn format_roman(degree: Degree, quality: &str) -> String {
    let display_quality = quality.replace("maj7", "M7").replace("ma7", "M7");
    format!("{degree}{display_quality}")
}

pub fn rewrite_symbol(
    symbol: &str,
    root_old: &str,
    root_new: Option<SpelledNote>,
    bass_old: Option<&str>,
    bass_new: Option<SpelledNote>,
) -> String {
    let mut rewritten = symbol.to_owned();
    if let Some(root_new) = root_new {
        if rewritten.starts_with(root_old) {
            rewritten = format!("{root_new}{}", &rewritten[root_old.len()..]);
        }
    }

    if let (Some((head, tail)), Some(bass_old), Some(bass_new)) =
        (rewritten.split_once('/'), bass_old, bass_new)
    {
        let tail = if let Some(remainder) = tail.strip_prefix(bass_old) {
            format!("{bass_new}{remainder}")
        } else {
            tail.to_owned()
        };
        rewritten = format!("{head}/{tail}");
    }
    rewritten
}

pub fn render_symbol(root: SpelledNote, quality: &str, bass: Option<SpelledNote>) -> String {
    match bass {
        Some(bass) => format!("{root}{quality}/{bass}"),
        None => format!("{root}{quality}"),
    }
}

pub fn split_note_and_suffix(text: &str) -> Option<(&str, &str)> {
    let first = text.chars().next()?;
    if !matches!(first.to_ascii_uppercase(), 'A'..='G') {
        return None;
    }
    let mut end = first.len_utf8();
    for ch in text[end..].chars() {
        if matches!(ch, '#' | 'b') {
            end += ch.len_utf8();
        } else {
            break;
        }
    }
    Some((&text[..end], &text[end..]))
}
