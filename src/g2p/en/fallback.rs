//! Last-resort letter-to-sound approximation for English words the
//! dictionary (`en_us.tsv`/`en_gb.tsv`) has never seen - mostly rare proper
//! nouns and neologisms. This is intentionally coarse: it exists so an
//! unknown word produces *some* plausible-sounding phonemes instead of
//! silence, not to compete with the dictionary on accuracy. Every symbol it
//! can emit is one of English's own phoneme/consonant set as used by the
//! dictionary data (misaki's `CONSONANTS`/`US_TAUS`), so output is always
//! safe to feed to the model.
//!
//! Longest-match digraph/trigraph rules first, then a single-letter table,
//! with primary stress placed on the first syllable - a reasonable default
//! for the short, simple words this path actually sees in practice.

/// Multi-letter patterns, longest first so e.g. "tion" wins over "ti"+"on".
const PATTERNS: &[(&str, &str)] = &[
    ("tion", "ʃən"),
    ("sion", "ʒən"),
    ("igh", "I"),
    ("augh", "ɔ"),
    ("eigh", "A"),
    ("ough", "ʌf"),
    ("ch", "ʧ"),
    ("sh", "ʃ"),
    ("th", "θ"),
    ("ph", "f"),
    ("wh", "w"),
    ("ck", "k"),
    ("ng", "ŋ"),
    ("qu", "kw"),
    ("ee", "i"),
    ("ea", "i"),
    ("oo", "u"),
    ("ai", "A"),
    ("ay", "A"),
    ("ow", "W"),
    ("ou", "W"),
    ("oy", "Y"),
    ("oi", "Y"),
    ("oa", "O"),
    ("ar", "ɑɹ"),
    ("er", "ɜɹ"),
    ("ir", "ɜɹ"),
    ("ur", "ɜɹ"),
    ("or", "ɔɹ"),
];

fn single_letter(c: char) -> Option<&'static str> {
    Some(match c {
        'a' => "æ",
        'e' => "ɛ",
        'i' => "ɪ",
        'o' => "ɑ",
        'u' => "ʌ",
        'y' => "ɪ",
        'b' => "b",
        'c' => "k",
        'd' => "d",
        'f' => "f",
        'g' => "ɡ",
        'h' => "h",
        'j' => "ʤ",
        'k' => "k",
        'l' => "l",
        'm' => "m",
        'n' => "n",
        'p' => "p",
        'q' => "k",
        'r' => "ɹ",
        's' => "s",
        't' => "t",
        'v' => "v",
        'w' => "w",
        'x' => "ks",
        'z' => "z",
        _ => return None,
    })
}

pub(crate) fn approximate(word: &str) -> String {
    let letters: Vec<char> = word
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphabetic())
        .collect();
    if letters.is_empty() {
        return String::new();
    }

    let mut out = String::new();
    out.push('\u{2c8}'); // primary stress on the first syllable
    let mut i = 0;
    'outer: while i < letters.len() {
        for &(pat, ph) in PATTERNS {
            let plen = pat.chars().count();
            if i + plen <= letters.len() && letters[i..i + plen].iter().collect::<String>() == pat
            {
                out.push_str(ph);
                i += plen;
                continue 'outer;
            }
        }
        if let Some(ph) = single_letter(letters[i]) {
            out.push_str(ph);
        }
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_empty_for_a_real_word() {
        assert!(!approximate("zzyzx").is_empty());
    }

    #[test]
    fn only_emits_known_vocabulary() {
        for word in ["blorptastic", "xylonqu", "zzyzxville", "a"] {
            let ps = approximate(word);
            for c in ps.chars() {
                assert!(
                    crate::g2p::is_known(c) || c == '\u{2c8}',
                    "{word} produced unknown phoneme {c:?} in {ps}"
                );
            }
        }
    }

    #[test]
    fn starts_with_primary_stress() {
        assert!(approximate("blorp").starts_with('\u{2c8}'));
    }
}
