//! Spanish grapheme-to-phoneme.
//!
//! Rendered as Latin American Spanish (seseo: `c`/`z` as /s/, not Castilian
//! /θ/), and without the b/d/g stop-vs-fricative allophony split down to
//! the syllable: the initial-vs-elsewhere approximation here gets the
//! common cases (word-initial, after a nasal/lateral) right, which covers
//! most instances in running text.

use super::{
    default_stress_run_start, is_unstressed_function_word, phonemize_words,
    secondary_stress_run_starts, strip_accent,
};

/// Articles, simple prepositions, coordinating conjunctions and clitic
/// pronouns: none of these carry a stress mark in connected speech.
const UNSTRESSED: &[&str] = &[
    "el", "la", "los", "las", "un", "una", "unos", "unas", "de", "en", "y", "o", "u", "que", "se",
    "le", "les", "lo", "me", "te", "nos", "mi", "mis", "tu", "tus", "su", "sus",
];

fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u')
}

fn phonemize_word(word: &str) -> String {
    let letters: Vec<char> = word.to_lowercase().chars().filter(|c| c.is_alphabetic()).collect();
    if letters.is_empty() {
        return String::new();
    }
    let stress_idx = if is_unstressed_function_word(word, UNSTRESSED) {
        None
    } else {
        default_stress_run_start(&letters)
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

        match c {
            'c' if next == Some('h') => {
                out.push('\u{2a7}'); // ʧ
                i += 2;
            }
            'c' if matches!(next.map(strip_accent), Some('e') | Some('i')) => {
                out.push('s');
                i += 1;
            }
            'c' => {
                out.push('k');
                i += 1;
            }
            'q' if next == Some('u') => {
                out.push('k'); // "qu" is always /k/, the u is silent
                i += 2;
            }
            'g' if next == Some('u') && matches!(next2.map(strip_accent), Some('e') | Some('i')) => {
                out.push('\u{261}'); // ɡ, u silent before e/i
                i += 2;
            }
            'g' if next == Some('u') && matches!(next2.map(strip_accent), Some('a') | Some('o')) => {
                out.push_str("\u{261}w");
                i += 2;
            }
            'g' if matches!(next.map(strip_accent), Some('e') | Some('i')) => {
                out.push('x'); // jota
                i += 1;
            }
            'g' => {
                out.push(if prev.is_none() || prev == Some('n') {
                    '\u{261}' // ɡ
                } else {
                    '\u{263}' // ɣ, intervocalic
                });
                i += 1;
            }
            'h' => {
                i += 1; // always silent
            }
            'l' if next == Some('l') => {
                out.push('\u{29d}'); // ʝ
                i += 2;
            }
            'r' if next == Some('r') => {
                out.push('r');
                i += 2;
            }
            'r' => {
                out.push(if prev.is_none() || matches!(prev, Some('n') | Some('l') | Some('s')) {
                    'r' // word-initial or after n/l/s: trilled
                } else {
                    '\u{27e}' // ɾ, tap
                });
                i += 1;
            }
            'ñ' => {
                out.push('\u{272}'); // ɲ
                i += 1;
            }
            'b' | 'v' => {
                out.push(if prev.is_none() || matches!(prev, Some('n') | Some('m')) {
                    'b'
                } else {
                    '\u{3b2}' // β
                });
                i += 1;
            }
            'd' => {
                out.push(if prev.is_none() || matches!(prev, Some('n') | Some('l')) {
                    'd'
                } else {
                    '\u{f0}' // ð
                });
                i += 1;
            }
            'y' => {
                if next.map(strip_accent).is_some_and(is_vowel) {
                    out.push('\u{29d}'); // ʝ, consonantal y
                } else {
                    out.push('i'); // vocalic y ("y", "hoy", "muy")
                }
                i += 1;
            }
            'x' => {
                out.push_str("ks");
                i += 1;
            }
            'j' => {
                out.push('x');
                i += 1;
            }
            'z' => {
                out.push('s'); // seseo
                i += 1;
            }
            's' | 't' | 'p' | 'f' | 'k' | 'l' | 'm' | 'n' | 'w' => {
                out.push(c);
                i += 1;
            }
            'a' | 'e' | 'i' | 'o' | 'u' | 'á' | 'é' | 'í' | 'ó' | 'ú' | 'ü' => {
                out.push(strip_accent(if c == 'ü' { 'u' } else { c }));
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
    fn ch_and_ll_become_single_phonemes() {
        assert!(phonemize_word("chico").starts_with('\u{2a7}'));
        assert!(phonemize_word("llamar").contains('\u{29d}'));
    }

    #[test]
    fn b_and_v_merge() {
        assert_eq!(
            phonemize_word("vamos").chars().nth(0),
            phonemize_word("bamos").chars().nth(0)
        );
    }

    #[test]
    fn stressed_accent_is_marked() {
        let ps = phonemize_word("camión");
        assert!(ps.contains('\u{2c8}'));
    }

    #[test]
    fn jota_sound_for_j_and_soft_g() {
        assert!(phonemize_word("jamón").starts_with('x'));
        assert!(phonemize_word("gente").contains('x'));
    }

    #[test]
    fn silent_h() {
        assert!(!phonemize_word("hola").contains('h'));
    }

    #[test]
    fn function_words_are_unstressed() {
        // Articles/prepositions carry no stress mark in connected speech.
        for word in ["el", "la", "de", "en", "y", "que", "mi", "su"] {
            let ps = phonemize_word(word);
            assert!(!ps.contains('\u{2c8}'), "{word} -> {ps:?}: should be unstressed");
        }
    }

    #[test]
    fn accented_homograph_is_still_stressed() {
        // "el" (article) is unstressed, but "él" (pronoun "he") is a
        // different word and must keep its stress.
        let ps = phonemize_word("él");
        assert!(ps.contains('\u{2c8}'), "{ps}: \u{00e9}l should still be stressed");
    }

    #[test]
    fn only_emits_known_vocabulary() {
        for word in ["hola", "cómo", "estás", "vamos", "parque", "año", "guerra", "pingüino"] {
            let ps = phonemize_word(word);
            for c in ps.chars() {
                assert!(crate::g2p::is_known(c), "{word} -> {ps} has unknown {c:?}");
            }
        }
    }
}
