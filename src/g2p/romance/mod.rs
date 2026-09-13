//! Rule-based grapheme-to-phoneme for the four Romance Kokoro languages
//! (Spanish, Italian, French, Brazilian Portuguese).
//!
//! Unlike English, these orthographies are close enough to phonemic that a
//! small hand-written rule set gets most words right without a dictionary:
//! spelling determines pronunciation almost completely (mainly exceptions
//! around loanwords and, in French, a fair amount of etymological silent
//! spelling). Each language's rules live in its own submodule; what's here is
//! shared plumbing.
//!
//! Known limitations, accepted for a first pass rather than pursued to
//! completion:
//!   * Stress placement approximates a syllable as one maximal run of vowel
//!     letters. This gets diphthongs right but mishandles genuine hiatus
//!     (two vowels pronounced as separate syllables, e.g. Spanish "país"),
//!     which a written accent mark corrects for anyway.
//!   * French liaison (a normally-silent final consonant surfacing before a
//!     vowel-initial word) is not implemented; words are phonemized in
//!     isolation.
//!   * Spanish is rendered as Latin American (seseo: c/z as /s/, not
//!     Castilian /θ/) since that is the more widely spoken variant absent a
//!     signal for which the caller wants.

pub(crate) mod es;
pub(crate) mod fr;
pub(crate) mod it;
pub(crate) mod pt;

use super::text;

const ACUTE_MAP: &[(char, char)] = &[
    ('á', 'a'), ('é', 'e'), ('í', 'i'), ('ó', 'o'), ('ú', 'u'),
    ('à', 'a'), ('è', 'e'), ('ì', 'i'), ('ò', 'o'), ('ù', 'u'),
    ('â', 'a'), ('ê', 'e'), ('î', 'i'), ('ô', 'o'), ('û', 'u'),
    ('ã', 'a'), ('õ', 'o'),
];

/// Whether `c` is a written accent/diacritic vowel that marks the stressed
/// syllable directly (Spanish/Italian/Portuguese acute/grave accents).
/// Portuguese's nasal `ã`/`õ` are excluded even though they carry a tilde,
/// because that mark is phonemic (nasalization) rather than a stress cue.
fn is_stress_accent(c: char) -> bool {
    matches!(c, 'á' | 'é' | 'í' | 'ó' | 'ú' | 'à' | 'è' | 'ì' | 'ò' | 'ù' | 'â' | 'ê' | 'î' | 'ô' | 'û')
}

/// Strip an accent mark down to its plain vowel letter, for languages where
/// the accent is purely a stress cue (not a distinct phoneme).
pub(crate) fn strip_accent(c: char) -> char {
    ACUTE_MAP.iter().find(|(a, _)| *a == c).map_or(c, |(_, b)| *b)
}

fn is_vowel_letter(c: char) -> bool {
    matches!(
        c,
        'a' | 'e' | 'i' | 'o' | 'u' | 'á' | 'é' | 'í' | 'ó' | 'ú' | 'à' | 'è' | 'ì' | 'ò' | 'ù'
            | 'â' | 'ê' | 'î' | 'ô' | 'û' | 'ã' | 'õ' | 'ü'
    )
}

/// Index (into `letters`) of the vowel-run that should carry primary stress,
/// for the Spanish/Italian/Portuguese "penultimate unless it ends in a
/// consonant other than n/s" rule. Returns `None` for words with no vowels.
///
/// `run_start` receives each vowel run's starting index, in order; the
/// chosen run is the one whose start this function returns.
pub(crate) fn default_stress_run_start(letters: &[char]) -> Option<usize> {
    default_stress_run_start_ending(letters, &['n', 's'])
}

/// Same rule, but with the set of word-final consonants that keep the
/// stress on the penultimate syllable (Spanish: n/s; Portuguese: m/s)
/// supplied by the caller, since the two languages mark the same "this is
/// not a special ending" role with different letters.
pub(crate) fn default_stress_run_start_ending(letters: &[char], keeps_penultimate: &[char]) -> Option<usize> {
    let runs = vowel_runs(letters);
    if runs.is_empty() {
        return None;
    }
    // A written accent anywhere overrides the default rule outright.
    for &(start, end) in &runs {
        if letters[start..end].iter().any(|&c| is_stress_accent(c)) {
            return Some(start);
        }
    }
    let last_letter = *letters.last().unwrap();
    // Portuguese words ending in a nasal vowel/diphthong ("irmã", "não",
    // "também") are oxytone - stressed on that final syllable - the opposite
    // of the general "ends in a vowel -> penultimate" default. Spanish has
    // no ã/õ letters, so this never fires for it.
    let ends_in_nasal_vowel = matches!(last_letter, 'ã' | 'õ')
        || letters.len() >= 2 && matches!(letters[letters.len() - 2], 'ã' | 'õ');
    let stress_final = ends_in_nasal_vowel
        || !(is_vowel_letter(last_letter) || keeps_penultimate.contains(&last_letter));
    if stress_final || runs.len() == 1 {
        Some(runs[runs.len() - 1].0)
    } else {
        Some(runs[runs.len() - 2].0)
    }
}

/// Like [`default_stress_run_start`], but for Italian: absent a written
/// accent, the default is always the penultimate syllable (parola piana),
/// regardless of what the word ends in - Spanish's "ends in a consonant
/// other than n/s" carve-out does not apply.
pub(crate) fn default_stress_run_start_penultimate(letters: &[char]) -> Option<usize> {
    let runs = vowel_runs(letters);
    if runs.is_empty() {
        return None;
    }
    for &(start, end) in &runs {
        if letters[start..end].iter().any(|&c| is_stress_accent(c)) {
            return Some(start);
        }
    }
    let idx = runs.len().saturating_sub(2);
    Some(runs[idx].0)
}

/// Maximal runs of vowel letters, as `(start, end)` half-open byte-index-free
/// char-index ranges.
pub(crate) fn vowel_runs(letters: &[char]) -> Vec<(usize, usize)> {
    let mut runs = Vec::new();
    let mut i = 0;
    while i < letters.len() {
        if is_vowel_letter(letters[i]) {
            let start = i;
            while i < letters.len() && is_vowel_letter(letters[i]) {
                i += 1;
            }
            runs.push((start, i));
        } else {
            i += 1;
        }
    }
    runs
}

/// Secondary-stress syllable starts (letter indices) for Spanish/Portuguese,
/// given the word's primary stress (a letter index, as returned by
/// [`default_stress_run_start`]/[`default_stress_run_start_ending`]).
///
/// On a standalone word (as opposed to one in a sentence, where
/// phrase-level effects also come into play), secondary stress falls on
/// every other syllable counting from the word's first ("elefante" ->
/// secondary on "e", primary on "fan"; "responsabilidad" -> secondary on
/// "res" and "sa", primary on "dad"), except a candidate immediately
/// adjacent to the primary syllable is dropped rather than clash with it
/// ("computadora": naively "ta" would be next in the alternation, but it is
/// adjacent to primary "do", so only "com" surfaces).
///
/// Italian and French do not show this pattern on standalone words at all -
/// only Spanish and Portuguese call this.
pub(crate) fn secondary_stress_run_starts(letters: &[char], primary_start: Option<usize>) -> Vec<usize> {
    let Some(primary_start) = primary_start else {
        return Vec::new();
    };
    let runs = vowel_runs(letters);
    let Some(primary_idx) = runs.iter().position(|&(s, _)| s == primary_start) else {
        return Vec::new();
    };
    runs.iter()
        .enumerate()
        .filter(|&(idx, _)| {
            idx % 2 == 0 && idx != primary_idx && idx.abs_diff(primary_idx) != 1
        })
        .map(|(_, &(start, _))| start)
        .collect()
}

/// Whether `word` is one of `list`'s unstressed function words - articles,
/// simple prepositions, conjunctions, clitic pronouns and the like, which in
/// connected Romance-language speech normally carry no stress of their own
/// (e.g. French "me", "vu", "dœ̃", "kə", "lə" and Italian "a", "la", "e",
/// "in" are all unstressed).
///
/// Marking every word - including these - with a stress mark the way the
/// per-word default rule otherwise would gives a sentence no peaks and
/// valleys: every syllable reads as equally prominent, which is a large part
/// of what makes generated speech sound flat/monotone rather than following
/// a sentence's natural rhythm. This is a plain lookup against the word as
/// written, so an accented homograph (Spanish "el" vs "él") is unaffected -
/// the accented spelling never matches an entry in these lists.
pub(crate) fn is_unstressed_function_word(word: &str, list: &[&str]) -> bool {
    list.contains(&word.to_lowercase().as_str())
}

/// Text -> phonemes, punctuation-preserving, phonemizing each whitespace-split
/// word with `word_fn`, joining with spaces - the shared word-loop every
/// Romance-language submodule uses.
pub(crate) fn phonemize_words(
    text_in: &str,
    word_fn: impl Fn(&str) -> String,
) -> Result<String, String> {
    text::phonemize_clauses(text_in, |run| {
        let out: Vec<String> = run.split_whitespace().map(&word_fn).collect();
        Ok(out.join(" "))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secondary_stress_alternates_from_the_start_skipping_the_primary_neighbor() {
        // "computadora": com-pu-ta-do-ra, primary on "do" (syllable 3,
        // 0-based). Naive alternation from the start would also flag
        // syllable 1 ("ta") - it sits right next to primary and is
        // suppressed to avoid a stress clash.
        let letters: Vec<char> = "computadora".chars().collect();
        let primary = default_stress_run_start(&letters); // "do" is the run at index 6..8? computed, not hand-picked
        let secondary = secondary_stress_run_starts(&letters, primary);
        // Only "com"'s run-start should survive as secondary.
        let runs = vowel_runs(&letters);
        let primary_idx = runs.iter().position(|&(s, _)| Some(s) == primary).unwrap();
        assert_eq!(primary_idx, 3, "sanity: primary should be the 4th syllable (do)");
        assert_eq!(secondary, vec![runs[0].0], "only the first syllable should get secondary");
    }

    #[test]
    fn accent_overrides_the_default_rule() {
        // "camión": "ió" is one syllable nucleus: the accent picks it out,
        // and the stress mark lands on the syllable's start, not the "ó"
        // specifically - "ca-MIÓN", not a mark buried mid-diphthong.
        let letters: Vec<char> = "camión".chars().collect();
        let start = default_stress_run_start(&letters).unwrap();
        assert_eq!(start, 3, "should pick the io syllable, not the first a");
        assert!(letters[start..].contains(&'ó'));
    }

    #[test]
    fn words_ending_in_vowel_stress_the_penultimate_syllable() {
        // "casa" is CA-sa: two syllables, so the penultimate one is the first.
        let letters: Vec<char> = "casa".chars().collect();
        let start = default_stress_run_start(&letters).unwrap();
        assert_eq!(start, 1, "stress goes on the first 'a', not the second");
    }

    #[test]
    fn words_ending_in_a_consonant_other_than_n_s_stress_the_final_syllable() {
        let letters: Vec<char> = "hotel".chars().collect();
        let start = default_stress_run_start(&letters).unwrap();
        assert_eq!(letters[start], 'e');
    }
}
