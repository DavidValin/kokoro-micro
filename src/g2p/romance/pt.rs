//! Brazilian Portuguese grapheme-to-phoneme.
//!
//! Simplifications accepted for a first pass: the open/closed vowel
//! distinction that written accents (á/â, é/ê, ó/ô) mark collapses to a
//! single quality per vowel letter (the accent still does its other job of
//! placing stress); intervocalic `s`/`x` voicing is approximated rather than
//! tracked precisely.

use super::{
    default_stress_run_start_ending, is_unstressed_function_word, phonemize_words,
    secondary_stress_run_starts, strip_accent,
};

/// Articles, simple (including contracted) prepositions, coordinating
/// conjunctions and clitic pronouns: the same closed classes left
/// unstressed in Spanish/Italian/French carry over directly to
/// Portuguese's equivalents.
const UNSTRESSED: &[&str] = &[
    "o", "a", "os", "as", "um", "uma", "uns", "umas", "de", "em", "e", "ou", "que", "se", "me",
    "te", "lhe", "lhes", "meu", "minha", "teu", "tua", "seu", "sua", "no", "na", "nos", "nas",
    "do", "da", "dos", "das", "ao", "aos", "à", "às",
];

fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u')
}

/// A vowel followed by `m`/`n` that is not itself followed by a vowel
/// nasalizes, and the `m`/`n` is silent - same shape as French, spelled with
/// different letters.
fn nasalizes(letters: &[char], i: usize) -> bool {
    matches!(letters[i], 'm' | 'n')
        && !letters.get(i + 1).copied().is_some_and(is_vowel)
}

fn phonemize_word(word: &str) -> String {
    let letters: Vec<char> = word.to_lowercase().chars().filter(|c| c.is_alphabetic()).collect();
    if letters.is_empty() {
        return String::new();
    }
    let stress_idx = if is_unstressed_function_word(word, UNSTRESSED) {
        None
    } else {
        default_stress_run_start_ending(&letters, &['m', 's'])
    };
    let secondary = secondary_stress_run_starts(&letters, stress_idx);
    let n = letters.len();
    let mut out = String::new();
    let mut i = 0;
    while i < n {
        if Some(i) == stress_idx {
            out.push('\u{2c8}');
        } else if secondary.contains(&i) {
            out.push('\u{2cc}');
        }
        let c = letters[i];
        let next = letters.get(i + 1).copied();
        let next2 = letters.get(i + 2).copied();
        let prev = if i > 0 { Some(letters[i - 1]) } else { None };

        // Written nasal vowels/diphthongs.
        if c == 'ã' && next == Some('o') {
            out.push('\u{251}');
            out.push('\u{303}');
            out.push('w');
            i += 2;
            continue;
        }
        if c == 'õ' && next == Some('e') {
            out.push('o');
            out.push('\u{303}');
            out.push('j');
            i += 2;
            continue;
        }
        if matches!(c, 'ã' | 'õ') {
            out.push(if c == 'ã' { 'a' } else { 'o' });
            out.push('\u{303}');
            i += 1;
            continue;
        }
        // "am"/"an"/"em"/"en"/"im"/"in"/"om"/"on"/"um"/"un" at a syllable's
        // end: nasalize the vowel, silence the m/n. Not when the n starts
        // "nh" ("manhã", "ninho"): that is the ɲ digraph below, a plain
        // consonant onset for the *next* syllable, not a nasalizer for
        // this one - "amanhã" must not lose that ɲ to a stray nasal vowel.
        if is_vowel(c)
            && next.is_some_and(|nx| {
                matches!(nx, 'm' | 'n')
                    && nasalizes(&letters, i + 1)
                    && !(nx == 'n' && letters.get(i + 2) == Some(&'h'))
            })
        {
            out.push(strip_accent(c));
            out.push('\u{303}');
            i += 2;
            continue;
        }

        match c {
            'n' if next == Some('h') => {
                out.push('\u{272}'); // ɲ
                i += 2;
            }
            'l' if next == Some('h') => {
                out.push('\u{28e}'); // ʎ
                i += 2;
            }
            'c' if next == Some('h') => {
                out.push('\u{283}'); // ʃ
                i += 2;
            }
            'c' if matches!(next.map(strip_accent), Some('e') | Some('i')) => {
                out.push('s');
                i += 1;
            }
            'ç' => {
                out.push('s');
                i += 1;
            }
            'c' => {
                out.push('k');
                i += 1;
            }
            'q' if next == Some('u') && matches!(next2.map(strip_accent), Some('e') | Some('i')) => {
                out.push('k');
                i += 2;
            }
            'q' if next == Some('u') => {
                out.push_str("kw");
                i += 2;
            }
            'g' if next == Some('u') && matches!(next2.map(strip_accent), Some('e') | Some('i')) => {
                out.push('\u{261}');
                i += 2;
            }
            'g' if next == Some('u') && matches!(next2.map(strip_accent), Some('a') | Some('o')) => {
                out.push_str("\u{261}w");
                i += 2;
            }
            'g' if matches!(next.map(strip_accent), Some('e') | Some('i')) => {
                out.push('\u{292}'); // ʒ
                i += 1;
            }
            'g' => {
                out.push('\u{261}');
                i += 1;
            }
            'x' => {
                out.push('\u{283}'); // ʃ, the common BP realization
                i += 1;
            }
            'j' => {
                out.push('\u{292}');
                i += 1;
            }
            'h' => {
                i += 1; // silent alone (nh/lh/ch handled above)
            }
            'r' if next == Some('r') => {
                out.push('h'); // BP "rr": realized as a glottal/velar fricative
                i += 2;
            }
            'r' if prev.is_none() => {
                out.push('h'); // BP word-initial r: same realization
                i += 1;
            }
            'r' => {
                out.push('\u{27e}'); // ɾ, tap elsewhere
                i += 1;
            }
            's' if prev.is_some_and(is_vowel) && next.is_some_and(is_vowel) => {
                out.push('z'); // intervocalic voicing
                i += 1;
            }
            'z' => {
                out.push('z');
                i += 1;
            }
            'l' if next.is_none() => {
                out.push('w'); // word-final l vocalizes, e.g. "Brasil"
                i += 1;
            }
            'a' | 'e' | 'i' | 'o' | 'u' | 'á' | 'â' | 'à' | 'é' | 'ê' | 'í' | 'ó' | 'ô' | 'ú' => {
                out.push(strip_accent(c));
                i += 1;
            }
            'b' | 'd' | 'f' | 'k' | 'l' | 'm' | 'n' | 'p' | 's' | 't' | 'v' | 'w' | 'y' => {
                out.push(c);
                i += 1;
            }
            _ => {
                i += 1;
            }
        }
    }
    out
}

pub(crate) fn phonemize(text: &str) -> Result<String, String> {
    phonemize_words(text, phonemize_word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nasal_diphthong_ao() {
        let ps = phonemize_word("não");
        assert!(ps.contains('\u{303}'));
    }

    #[test]
    fn vowel_before_final_m_nasalizes() {
        let ps = phonemize_word("bem");
        assert!(ps.contains('\u{303}'));
        assert!(!ps.contains('m'));
    }

    #[test]
    fn nh_digraph_survives_after_a_vowel() {
        // "amanhã": the "nh" in the middle is the ɲ digraph, a plain
        // consonant onset - not a vowel-nasalizing n the way the final
        // "nhã" IS nasal (written with ã). Must still contain ɲ.
        let ps = phonemize_word("amanhã");
        assert!(ps.contains('\u{272}'), "{ps}: missing ɲ from \"nh\"");
    }

    #[test]
    fn word_final_l_vocalizes() {
        assert!(phonemize_word("brasil").ends_with('w'));
    }

    #[test]
    fn initial_r_is_a_fricative_not_a_tap() {
        assert!(phonemize_word("rato").starts_with('h'));
    }

    #[test]
    fn nasal_ending_words_stress_the_final_syllable() {
        let ps = phonemize_word("também");
        // Stress mark should be on the last syllable (the nasal one), i.e.
        // closer to the end of the string than the start.
        let pos = ps.find('\u{2c8}').unwrap();
        assert!(pos > ps.len() / 2, "{ps}: expected final-syllable stress");
    }

    #[test]
    fn function_words_are_unstressed() {
        for word in ["o", "a", "de", "em", "e", "que", "seu"] {
            let ps = phonemize_word(word);
            assert!(!ps.contains('\u{2c8}'), "{word} -> {ps:?}: should be unstressed");
        }
    }

    #[test]
    fn only_emits_known_vocabulary() {
        for word in ["não", "também", "brasil", "irmã", "coração", "olá", "obrigado"] {
            let ps = phonemize_word(word);
            for c in ps.chars() {
                assert!(crate::g2p::is_known(c), "{word} -> {ps} has unknown {c:?}");
            }
        }
    }
}
