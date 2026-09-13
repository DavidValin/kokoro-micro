//! Punctuation-preserving clause splitting, shared by every G2P backend that
//! phonemizes one word/clause at a time (everything except Mandarin/Japanese,
//! which tokenize differently).
//!
//! Kokoro's vocabulary has tokens for punctuation, so rather than discard it
//! before phonemizing (which is what a naive per-word G2P would do), text is
//! split into clause-sized runs on punctuation boundaries and the boundary
//! character is re-emitted between the phonemized runs. This is the same
//! effect as misaki's `preserve_punctuation=True`.

/// Whether a `.` or `,` at `i` sits between two digits, i.e. it is a decimal
/// point or a thousands separator rather than punctuation.
///
/// Splitting there would hand the phonemizer the halves separately, and
/// "1,234" would become "one, two hundred thirty four" instead of "one
/// thousand two hundred and thirty four".
pub(crate) fn is_inside_number(chars: &[char], i: usize) -> bool {
    matches!(chars.get(i), Some('.') | Some(','))
        && i > 0
        && chars[i - 1].is_ascii_digit()
        && chars.get(i + 1).is_some_and(char::is_ascii_digit)
}

/// Punctuation the model has tokens for, and what we normalize it to.
pub(crate) fn punctuation_token(c: char) -> Option<char> {
    Some(match c {
        ';' => ';',
        ':' => ':',
        ',' => ',',
        '.' => '.',
        '!' => '!',
        '?' => '?',
        '—' | '–' => '—',
        '…' => '…',
        '"' => '"',
        '(' | '[' | '{' => '(',
        ')' | ']' | '}' => ')',
        '\u{201c}' | '«' => '\u{201c}',
        '\u{201d}' | '»' => '\u{201d}',
        // Devanagari danda / double danda: Hindi's own sentence-final
        // punctuation, not the ASCII period - without this, Hindi text
        // loses every sentence break the model would otherwise pause on.
        '\u{964}' | '\u{965}' => '.',
        _ => return None,
    })
}

/// Text -> Kokoro phonemes, preserving punctuation across clause boundaries.
///
/// `word_phonemizer` is called once per punctuation-free run of text (already
/// trimmed) and must return that run's phonemes in Kokoro's alphabet.
pub(crate) fn phonemize_clauses(
    text: &str,
    mut word_phonemizer: impl FnMut(&str) -> Result<String, String>,
) -> Result<String, String> {
    let mut out = String::new();
    let mut run = String::new();

    let mut flush = |run: &mut String, out: &mut String| -> Result<(), String> {
        if run.trim().is_empty() {
            run.clear();
            return Ok(());
        }
        let ps = word_phonemizer(run.trim())?;
        let ps = ps.trim();
        if !ps.is_empty() {
            // No space after an opening bracket or quote, which belongs to
            // what follows it.
            let opens = matches!(out.chars().last(), Some('(') | Some('\u{201c}'));
            if !out.is_empty() && !out.ends_with(' ') && !opens {
                out.push(' ');
            }
            out.push_str(ps);
        }
        run.clear();
        Ok(())
    };

    let chars: Vec<char> = text.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        match punctuation_token(c) {
            Some(_) if is_inside_number(&chars, i) => run.push(c),
            Some(p) => {
                flush(&mut run, &mut out)?;
                if matches!(p, '(' | '\u{201c}') && !out.is_empty() && !out.ends_with(' ') {
                    out.push(' ');
                }
                out.push(p);
            }
            None => run.push(c),
        }
    }
    flush(&mut run, &mut out)?;
    Ok(out.trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_separators_are_not_clause_breaks() {
        let chars: Vec<char> = "1,234.5 and, then.".chars().collect();
        assert!(is_inside_number(&chars, 1), "thousands separator");
        assert!(is_inside_number(&chars, 5), "decimal point");
        assert!(!is_inside_number(&chars, 11), "a real comma");
        assert!(!is_inside_number(&chars, 17), "a real full stop");
    }

    #[test]
    fn punctuation_is_reinserted_between_phonemized_runs() {
        let ps = phonemize_clauses("hi, there.", |run| Ok(run.to_uppercase())).unwrap();
        assert_eq!(ps, "HI, THERE.");
    }

    #[test]
    fn devanagari_danda_is_a_sentence_break() {
        // "है। हम" - danda between two words must split them into separate
        // clauses, the same as an ASCII period would, not vanish silently.
        let ps = phonemize_clauses("है। हम", |run| Ok(run.to_string())).unwrap();
        assert_eq!(ps, "है. हम");
    }
}
