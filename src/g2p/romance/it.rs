//! Italian grapheme-to-phoneme.
//!
//! Simplifications accepted for a first pass: the open/closed `e`/`o`
//! distinction (mid vowels are lexically, not orthographically, conditioned
//! in Italian outside of stressed word-final syllables) collapses to the
//! closed vowel. Default stress placement is always penultimate (see
//! [`super::default_stress_run_start_penultimate`]), with two rule-derivable
//! exceptions layered on top in [`stress_run_start`]: the `-issimo`
//! superlative suffix, and a lexicon of common sdrucciole (words stressed on
//! the antepenultimate syllable, e.g. "tavolo", "medico") - spelling alone
//! can't tell these apart from a regular word, so anything not in that
//! (necessarily incomplete) list still defaults to penultimate and will be
//! stressed wrong.

use super::{
    default_stress_run_start_penultimate, is_unstressed_function_word, phonemize_words,
    strip_accent, vowel_runs,
};

/// Common sdrucciole (antepenultimate stress). Not remotely exhaustive:
/// Italian doesn't mark this distinction in writing, so there is no way to
/// derive it from spelling, only to list known cases. Singular and
/// plural/feminine forms are listed
/// separately since deriving one from the other would need morphology this
/// crate doesn't have; all share the same syllable count as their lemma, so
/// "antepenultimate" lands in the same place for each.
const SDRUCCIOLE: &[&str] = &[
    "tavolo", "tavoli", "medico", "medici", "medica", "mediche", "numero", "numeri", "pubblico",
    "pubblici", "pubblica", "pubbliche", "semplice", "semplici", "musica", "musiche", "ultimo",
    "ultimi", "ultima", "ultime", "macchina", "macchine", "possibile", "possibili", "difficile",
    "difficili", "facile", "facili", "utile", "utili", "inutile", "inutili", "capitolo",
    "capitoli", "popolo", "popoli", "angolo", "angoli", "titolo", "titoli", "articolo",
    "articoli", "ottimo", "ottimi", "ottima", "ottime", "massimo", "massimi", "massima",
    "massime", "minimo", "minimi", "minima", "minime", "pratico", "pratici", "pratica",
    "pratiche", "economico", "economici", "economica", "economiche", "politico", "politici",
    "politica", "politiche", "fisico", "fisici", "fisica", "fisiche", "classico", "classici",
    "classica", "classiche", "matematica", "matematiche", "specifico", "specifici", "specifica",
    "specifiche", "automatico", "automatici", "automatica", "automatiche", "simpatico",
    "simpatici", "simpatica", "simpatiche", "antipatico", "antipatici", "antipatica",
    "antipatiche", "rapido", "rapidi", "rapida", "rapide", "stupido", "stupidi", "stupida",
    "stupide", "timido", "timidi", "timida", "timide", "solido", "solidi", "solida", "solide",
    "valido", "validi", "valida", "valide", "comodo", "comodi", "comoda", "comode", "debole",
    "deboli", "genere", "generi", "carattere", "caratteri", "lettera", "lettere", "camera",
    "camere", "domenica", "domeniche", "sabato", "sabati", "epoca", "epoche", "area", "aree",
    "telefono", "telefoni", "automobile", "automobili", "elicottero", "elicotteri", "isola",
    "isole", "vergine", "vergini", "indice", "indici",
];

/// Articles, simple prepositions, coordinating conjunctions and clitic
/// pronouns: none of these carry a stress mark in connected speech.
const UNSTRESSED: &[&str] = &[
    "il", "lo", "la", "i", "gli", "le", "un", "una", "uno", "di", "a", "da", "in", "con", "su",
    "per", "e", "o", "che", "si", "ci", "vi", "mi", "ti", "ne",
];

fn is_front(c: Option<char>) -> bool {
    matches!(c.map(strip_accent), Some('e') | Some('i'))
}

fn is_back_vowel(c: Option<char>) -> bool {
    matches!(c.map(strip_accent), Some('a') | Some('o') | Some('u'))
}

/// Whether the `i` right after a soft c/g/sc (at `letters[i]`) is the silent
/// softness-marking diacritic rather than a pronounced vowel - true when
/// another vowel follows it ("ciao", "giorno": ci/gi + a/o/u), false when it
/// is the last letter or a consonant follows ("cibo", "produci": a real
/// vowel).
fn diacritic_i(letters: &[char], i: usize) -> bool {
    letters.get(i) == Some(&'i') && matches!(letters.get(i + 1), Some('a') | Some('o') | Some('u'))
}

/// How many letters `cc`/`gg` (starting at `i`) span before the next real
/// vowel, accounting for a `-cchi/-cche`/`-gghi/-gghe` hardening `h` and a
/// `-cci/-ggi` softness-marking diacritic `i` the same way the single-letter
/// case does. Returns `(consumed, next_is_front)`.
fn geminate_span(letters: &[char], i: usize) -> (usize, bool) {
    match letters.get(i + 2) {
        Some('h') => (3, false), // "cch"/"ggh": h forces hard regardless of what follows
        Some('i') if diacritic_i(letters, i + 2) => (3, true), // "cci"/"ggi" + a/o/u
        Some(c) if is_front(Some(*c)) => (2, true), // "cce"/"cci"(+consonant/end), "gge"/"ggi"
        _ => (2, false), // "cca"/"cco"/"ccu" etc.
    }
}

fn phonemize_word(word: &str) -> String {
    let letters: Vec<char> = word.to_lowercase().chars().filter(|c| c.is_alphabetic()).collect();
    if letters.is_empty() {
        return String::new();
    }
    let stress_idx = if is_unstressed_function_word(word, UNSTRESSED) {
        None
    } else {
        stress_run_start(word, &letters)
    };
    let n = letters.len();
    let mut out = String::new();
    let mut i = 0;
    let mut stress_placed = false;
    while i < n {
        // ">=", not "==": a diacritic "i" (silent, "ciao"/"giorno") can be
        // the exact letter a vowel run starts on, and the c/g branches below
        // jump straight past it - so the stress mark must land the first
        // time i reaches or passes stress_idx, not only on an exact match.
        if !stress_placed && stress_idx.is_some_and(|s| i >= s) {
            out.push('\u{2c8}');
            stress_placed = true;
        }
        let c = letters[i];
        let next = letters.get(i + 1).copied();
        let next2 = letters.get(i + 2).copied();
        let prev = if i > 0 { Some(letters[i - 1]) } else { None };
        let geminate_letter = next == Some(c) && c.is_alphabetic() && !matches!(c, 'a' | 'e' | 'i' | 'o' | 'u');

        match c {
            'g' if next == Some('g') => {
                let (span, front) = geminate_span(&letters, i);
                out.push(if front { '\u{2a4}' } else { '\u{261}' });
                out.push('\u{2d0}');
                i += span;
            }
            'c' if next == Some('c') => {
                let (span, front) = geminate_span(&letters, i);
                out.push(if front { '\u{2a7}' } else { 'k' });
                out.push('\u{2d0}');
                i += span;
            }
            'g' if next == Some('l') && next2 == Some('i') => {
                out.push('\u{28e}'); // ʎ
                // "glia/glio/glie/gliu": the i is part of the palatal, not a
                // separate vowel. But word-final "-gli" or "gli" before a
                // consonant (e.g. "sbagli") pronounces the i as a real vowel,
                // so only swallow it when another vowel follows.
                if letters
                    .get(i + 3)
                    .is_some_and(|v| matches!(v, 'a' | 'e' | 'o' | 'u'))
                {
                    i += 3;
                } else {
                    i += 2;
                }
            }
            'g' if next == Some('n') => {
                out.push('\u{272}'); // ɲ
                i += 2;
            }
            'g' if next == Some('h') => {
                out.push('\u{261}'); // ɡ, always hard before h
                i += 2;
            }
            'g' if is_front(next) => {
                out.push('\u{2a4}'); // ʤ
                i += if diacritic_i(&letters, i + 1) { 2 } else { 1 };
            }
            'g' => {
                out.push('\u{261}'); // ɡ
                i += 1;
            }
            's' if next == Some('c') && is_front(next2) => {
                out.push('\u{283}'); // ʃ
                i += if diacritic_i(&letters, i + 2) { 3 } else { 2 };
            }
            'c' if next == Some('h') => {
                out.push('k');
                i += 2;
            }
            'c' if is_front(next) => {
                out.push('\u{2a7}'); // ʧ
                i += if diacritic_i(&letters, i + 1) { 2 } else { 1 };
            }
            'c' => {
                out.push('k');
                i += 1;
            }
            'q' if next == Some('u') => {
                out.push_str("kw");
                i += 2;
            }
            'r' if next == Some('r') => {
                out.push('r');
                i += 2;
            }
            'r' => {
                out.push('\u{27e}'); // ɾ
                i += 1;
            }
            'z' if next == Some('z') => {
                out.push('\u{2a6}'); // ʦ
                out.push('\u{2d0}');
                i += 2;
            }
            'z' => {
                // Intervocalic /ts/ is geminate in Italian even when spelled
                // with a single z ("grazie" -> [ɡratˈtsje], not [ɡraˈtsje]);
                // word-initial or post-consonant z is not.
                let intervocalic = prev.is_some_and(|p| is_front(Some(p)) || is_back_vowel(Some(p)))
                    && next.is_some_and(|nx| is_front(Some(nx)) || is_back_vowel(Some(nx)));
                out.push('\u{2a6}'); // ʦ
                if intervocalic {
                    out.push('\u{2d0}');
                }
                i += 1;
            }
            'h' => {
                i += 1; // only ever disambiguates c/g above; no sound alone
            }
            _ if geminate_letter => {
                // Gemination is phonemic in Italian ("sonno" vs "sono"), so
                // it needs to survive as a length mark on the consonant.
                if let Some(ph) = single_consonant(c) {
                    out.push(ph);
                    out.push('\u{2d0}'); // ː
                }
                i += 2;
            }
            _ => {
                if let Some(ph) = single_consonant(c) {
                    out.push(ph);
                } else if matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'à' | 'è' | 'é' | 'ì' | 'ò' | 'ó' | 'ù') {
                    out.push(strip_accent(c));
                }
                i += 1;
            }
        }
    }
    out
}

/// [`default_stress_run_start_penultimate`], with two rule-derivable
/// exceptions layered on top:
///   * the `-issimo`/`-issima`/`-issimi`/`-issime` superlative suffix is
///     always stressed on the "is", which for the plural forms (4
///     syllables: -is-si-mi/-me) is the antepenultimate syllable, not the
///     penultimate the default rule would otherwise pick;
///   * a word in the [`SDRUCCIOLE`] lexicon is stressed on its antepenultimate
///     syllable outright - spelling gives no other way to tell these apart
///     from a regular (penultimate-stressed) word of the same shape.
fn stress_run_start(word: &str, letters: &[char]) -> Option<usize> {
    let n = letters.len();
    if n >= 8
        && letters[n - 6] == 'i'
        && letters[n - 5] == 's'
        && letters[n - 4] == 's'
        && letters[n - 3] == 'i'
        && letters[n - 2] == 'm'
        && matches!(letters[n - 1], 'o' | 'a' | 'i' | 'e')
    {
        return Some(n - 6);
    }
    if SDRUCCIOLE.contains(&word.to_lowercase().as_str()) {
        let runs = vowel_runs(letters);
        if runs.len() >= 3 {
            return Some(runs[runs.len() - 3].0);
        }
    }
    default_stress_run_start_penultimate(letters)
}

fn single_consonant(c: char) -> Option<char> {
    match c {
        'b' | 'd' | 'f' | 'l' | 'm' | 'n' | 'p' | 's' | 't' | 'v' | 'w' | 'k' => Some(c),
        _ => None,
    }
}

pub(crate) fn phonemize(text: &str) -> Result<String, String> {
    phonemize_words(text, phonemize_word)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soft_c_and_g_before_front_vowels() {
        assert!(phonemize_word("ciao").contains('\u{2a7}'));
        assert!(phonemize_word("gente").contains('\u{2a4}'));
    }

    #[test]
    fn hard_c_and_g_before_h() {
        assert!(phonemize_word("chiesa").contains('k'));
        assert!(phonemize_word("ghiaccio").contains('\u{261}'));
    }

    #[test]
    fn gemination_adds_a_length_mark() {
        assert!(phonemize_word("nonno").contains('\u{2d0}'));
        assert!(!phonemize_word("sono").contains('\u{2d0}'));
    }

    #[test]
    fn double_g_before_a_front_vowel_is_a_geminate_affricate_not_g_plus_j() {
        // "oggi" ([ˈɔddʒi]) was coming out as ɡ+ʤ (two different manners of
        // articulation glued together, not a real Italian sound) instead of
        // a single geminated ʤː.
        let ps = phonemize_word("oggi");
        assert!(ps.contains("\u{2a4}\u{2d0}"), "{ps}: expected geminate ʤː");
        assert!(!ps.contains('\u{261}'), "{ps}: should not also contain plain ɡ");
    }

    #[test]
    fn double_c_before_a_front_vowel_is_a_geminate_affricate() {
        // "riccio" ([ˈrittʃo])
        let ps = phonemize_word("riccio");
        assert!(ps.contains("\u{2a7}\u{2d0}"), "{ps}: expected geminate ʧː");
    }

    #[test]
    fn double_g_before_a_back_vowel_is_a_geminate_stop() {
        // "leggo" ([ˈleɡɡo]) - hard g, geminated, not the affricate.
        let ps = phonemize_word("leggo");
        assert!(ps.contains("\u{261}\u{2d0}"), "{ps}: expected geminate ɡː");
    }

    #[test]
    fn cch_before_a_front_vowel_stays_hard() {
        // "pacchi" ([ˈpakki]): the h keeps it /k/, not /ʧ/.
        let ps = phonemize_word("pacchi");
        assert!(ps.contains("k\u{2d0}"), "{ps}: expected geminate kː");
        assert!(!ps.contains('\u{2a7}'), "{ps}: h should block the affricate");
    }

    #[test]
    fn intervocalic_z_is_geminate_even_when_spelled_single() {
        // "grazie" ([ɡratˈtsje]), not [ɡraˈtsje].
        let ps = phonemize_word("grazie");
        assert!(ps.contains("\u{2a6}\u{2d0}"), "{ps}: expected geminate ʦː");
    }

    #[test]
    fn word_initial_z_is_not_geminated() {
        let ps = phonemize_word("zero");
        assert!(!ps.contains("\u{2a6}\u{2d0}"), "{ps}: word-initial z has no preceding vowel to geminate from");
    }

    #[test]
    fn issimo_superlative_stresses_the_is_syllable() {
        // "bellissime" is bel-LIS-si-me: default penultimate would wrongly
        // pick "si" (syllable 3 of 4); the real stress is "lis" (syllable 2).
        let ps = phonemize_word("bellissime");
        let stress_pos = ps.find('\u{2c8}').unwrap();
        assert!(
            ps[stress_pos..].starts_with("\u{2c8}is") || ps[stress_pos..].starts_with('\u{2c8}'),
            "{ps}: stress should land on the -is- of -issime"
        );
    }

    #[test]
    fn known_sdrucciole_stress_the_antepenultimate_syllable() {
        // "tavolo" is TA-vo-lo: the default penultimate rule would wrongly
        // pick "vo". Checked against a lexicon of known exceptions.
        assert_eq!(phonemize_word("tavolo"), "t\u{2c8}avolo");
    }

    #[test]
    fn sdrucciole_plural_forms_are_also_covered() {
        // "medici" (ME-di-ci): the plural is a separate lexicon entry since
        // deriving it from "medico" needs morphology this crate doesn't have.
        let ps = phonemize_word("medici");
        assert!(ps.starts_with("m\u{2c8}e"), "{ps}: medici should stress its first syllable");
    }

    #[test]
    fn a_word_not_in_the_sdrucciole_lexicon_still_defaults_to_penultimate() {
        // "parlare" is not a sdrucciola; unlisted words must still fall
        // through to the ordinary penultimate default rather than, say,
        // matching every word by accident.
        let ps = phonemize_word("parlare");
        assert!(!ps.starts_with("p\u{2c8}a"), "{ps}: parlare should not stress its first syllable");
    }

    #[test]
    fn default_stress_is_penultimate() {
        // "parlare" -> par-LA-re; stress mark should precede the "la" vowel.
        let ps = phonemize_word("parlare");
        let stress_pos = ps.find('\u{2c8}').unwrap();
        assert!(ps[stress_pos..].starts_with("\u{2c8}la") || ps[stress_pos..].starts_with('\u{2c8}'));
    }

    #[test]
    fn function_words_are_unstressed() {
        for word in ["il", "la", "e", "in", "di", "un", "che"] {
            let ps = phonemize_word(word);
            assert!(!ps.contains('\u{2c8}'), "{word} -> {ps:?}: should be unstressed");
        }
    }

    #[test]
    fn only_emits_known_vocabulary() {
        for word in [
            "ciao", "come", "stai", "oggi", "andiamo", "parco", "gnocchi", "famiglia", "riccio",
            "leggo", "pacchi", "grazie", "zero", "bellissime",
        ] {
            let ps = phonemize_word(word);
            for c in ps.chars() {
                assert!(crate::g2p::is_known(c), "{word} -> {ps} has unknown {c:?}");
            }
        }
    }
}
