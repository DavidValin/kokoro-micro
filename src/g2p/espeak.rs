//! eSpeak NG front end for the seven non-CJK Kokoro languages.
//!
//! This talks to `espeak-ng` through `espeak-rs-sys` directly rather than
//! through the `espeak-rs` wrapper, because two things the model needs are not
//! reachable through that wrapper's API:
//!
//! * **Tie characters.** Kokoro represents diphthongs and affricates as single
//!   tokens (`A` = /eɪ/, `I` = /aɪ/, `O` = /oʊ/, `W` = /aʊ/, `Y` = /ɔɪ/,
//!   `ʧ`, `ʤ`, `ʦ`). To rewrite them reliably we have to know where eSpeak
//!   considers one phoneme to end and the next to begin, which is what
//!   `phonememode` bit 7 asks for - it makes eSpeak write `e^ɪ` instead of the
//!   ambiguous `eɪ`. Without it, every diphthong reaches the model as two
//!   tokens it was barely trained on.
//! * **Punctuation.** `espeak_TextToPhonemes` returns one clause per call and
//!   reports the clause terminator through a separate out-parameter that this
//!   version of espeak-ng does not export. Rather than guess, we split the
//!   text on punctuation ourselves and re-emit it, which is what misaki's
//!   `preserve_punctuation=True` amounts to.
//!
//! The rewrite tables are ports of `misaki/espeak.py` (`EspeakG2P` for
//! Spanish/French/Hindi/Italian/Portuguese, `EspeakFallback` for English).

use espeak_rs_sys as sys;
use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use super::Lang;

/// eSpeak keeps the selected voice in global state, so every call that sets a
/// voice and reads phonemes back has to be serialized.
static ESPEAK: OnceLock<Result<Mutex<()>, String>> = OnceLock::new();

/// Environment variables that name the directory *containing* `espeak-ng-data`.
/// The first is piper's convention, which several Rust espeak wrappers follow;
/// the second is espeak-ng's own.
const DATA_DIR_ENVS: [&str; 2] = ["PIPER_ESPEAKNG_DATA_DIRECTORY", "ESPEAK_DATA_PATH"];
const DATA_DIR_NAME: &str = "espeak-ng-data";

/// Prefixes a system install of espeak-ng typically uses.
const SYSTEM_PREFIXES: [&str; 7] = [
    "/usr/share",
    "/usr/local/share",
    "/usr/lib",
    "/usr/lib/x86_64-linux-gnu",
    "/usr/lib/aarch64-linux-gnu",
    "/opt/homebrew/share",
    "/opt/local/share",
];

/// The tie character eSpeak inserts inside multi-letter phoneme names.
const TIE: char = '^';

/// A directory only counts if the data is actually in it. espeak-ng reports a
/// missing `phontab` on stderr and then fails initialization with a code that
/// says nothing about why, so check before handing it a path.
fn has_data(dir: &Path) -> bool {
    dir.join(DATA_DIR_NAME).join("phontab").is_file()
}

/// Every place we are willing to look for `espeak-ng-data`, in order.
///
/// Deliberately thorough, because the one path espeak-ng falls back to on its
/// own is the worst of the lot: `espeak-rs-sys` compiles in the `OUT_DIR` of
/// whatever build produced the library, which is a path under `target/` that
/// does not survive a `cargo clean`, a dependency bump, or moving the binary
/// to another machine.
fn data_dir_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut push = |p: PathBuf| {
        if !candidates.contains(&p) {
            candidates.push(p);
        }
    };

    for var in DATA_DIR_ENVS {
        if let Some(dir) = std::env::var_os(var) {
            push(PathBuf::from(dir));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        push(cwd);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            push(dir.to_path_buf());
            // `target/release/examples/foo` -> `target/release`
            if let Some(parent) = dir.parent() {
                push(parent.to_path_buf());
            }
        }
    }
    if let Some(build_dir) = option_env!("KOKORO_ESPEAK_BUILD_DATA_DIR") {
        push(PathBuf::from(build_dir));
    }
    for prefix in SYSTEM_PREFIXES {
        push(PathBuf::from(prefix));
    }
    candidates
}

/// The first candidate that actually holds the data.
fn data_dir() -> Option<PathBuf> {
    data_dir_candidates().into_iter().find(|d| has_data(d))
}

fn engine() -> Result<&'static Mutex<()>, String> {
    ESPEAK
        .get_or_init(|| {
            let found = data_dir();
            // Held for the lifetime of the process: espeak keeps the pointer.
            let path = found
                .as_ref()
                .and_then(|p| CString::new(p.to_string_lossy().as_bytes().to_vec()).ok());
            let path_ptr = path.as_ref().map_or(std::ptr::null(), |c| c.as_ptr());
            let rate = unsafe {
                sys::espeak_Initialize(
                    sys::espeak_AUDIO_OUTPUT_AUDIO_OUTPUT_RETRIEVAL,
                    0,
                    path_ptr,
                    sys::espeakINITIALIZE_DONT_EXIT as c_int,
                )
            };
            std::mem::forget(path);
            if rate <= 0 {
                let tried = data_dir_candidates()
                    .iter()
                    .map(|p| format!("  {}", p.join(DATA_DIR_NAME).display()))
                    .collect::<Vec<_>>()
                    .join("\n");
                return Err(format!(
                    "could not initialize eSpeak NG: no `{DATA_DIR_NAME}` found. Install \
                     espeak-ng, or set {} to the directory containing it.\nLooked in:\n{tried}",
                    DATA_DIR_ENVS[0]
                ));
            }
            Ok(Mutex::new(()))
        })
        .as_ref()
        .map_err(|e| e.clone())
}

/// Select the first of `candidates` this espeak-ng installation knows.
///
/// Caller must hold the engine lock. The answer is cached: resolution is a
/// scan of the voice directory, and it cannot change while we are running.
fn select_voice(candidates: &[&'static str]) -> Result<&'static str, String> {
    static RESOLVED: OnceLock<Mutex<HashMap<&'static str, &'static str>>> = OnceLock::new();
    let cache = RESOLVED.get_or_init(|| Mutex::new(HashMap::new()));

    let key = *candidates
        .first()
        .ok_or_else(|| "no eSpeak voice for this language".to_string())?;
    if let Ok(map) = cache.lock() {
        if let Some(found) = map.get(key) {
            let name = CString::new(*found).expect("checked name");
            unsafe { sys::espeak_SetVoiceByName(name.as_ptr()) };
            return Ok(found);
        }
    }

    for candidate in candidates {
        let Ok(name) = CString::new(*candidate) else {
            continue;
        };
        if unsafe { sys::espeak_SetVoiceByName(name.as_ptr()) } == sys::espeak_ERROR_EE_OK {
            if let Ok(mut map) = cache.lock() {
                map.insert(key, candidate);
            }
            return Ok(candidate);
        }
    }
    Err(format!(
        "eSpeak NG has none of these voices installed: {candidates:?}"
    ))
}

/// eSpeak marks a switch to another language's rules inline, as `(en)`. Real
/// parentheses never reach espeak - [`phonemize`] peels punctuation off first -
/// so anything parenthesized in the output is one of these flags.
fn strip_language_flags(ps: &str) -> String {
    let mut out = String::with_capacity(ps.len());
    let mut depth = 0usize;
    for c in ps.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

/// Ask eSpeak for the IPA of one punctuation-free run of text.
fn phonemize_run(text: &str, candidates: &[&'static str]) -> Result<String, String> {
    let lock = engine()?;
    let _guard = lock.lock().map_err(|_| "eSpeak lock poisoned".to_string())?;
    select_voice(candidates)?;

    // bits 0-2 = 2 -> IPA output; bit 7 -> use bits 8-23 as a tie character.
    let phoneme_mode: c_int =
        (sys::espeakINITIALIZE_PHONEME_IPA | 0x80 | ((TIE as u32) << 8)) as c_int;

    let text_c = CString::new(text).map_err(|_| "text contains a NUL byte".to_string())?;
    let mut cursor = text_c.as_ptr() as *const c_void;
    let mut out = String::new();
    while !cursor.is_null() {
        let ptr = unsafe {
            sys::espeak_TextToPhonemes(
                &mut cursor as *mut *const c_void,
                sys::espeakCHARS_UTF8 as c_int,
                phoneme_mode,
            )
        };
        if ptr.is_null() {
            break;
        }
        out.push_str(&unsafe { CStr::from_ptr(ptr as *const c_char) }.to_string_lossy());
    }
    Ok(strip_language_flags(&out))
}

/// Rewrite rules applied to eSpeak's tie-marked IPA, longest pattern first.
///
/// Applied in a single left-to-right scan so that a rule never rewrites the
/// output of another - the tie-bearing patterns must win over the bare-letter
/// ones (`e^ə` -> `ɛː` before `e` -> `A`), which a naive sequence of
/// whole-string replacements gets wrong.
fn rewrite(input: &str, rules: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;
    'outer: while !rest.is_empty() {
        for (from, to) in rules {
            if rest.starts_with(from) {
                out.push_str(to);
                rest = &rest[from.len()..];
                continue 'outer;
            }
        }
        let c = rest.chars().next().expect("non-empty");
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

/// Shared misaki rewrites: the tokens Kokoro spells as one symbol.
/// Port of `EspeakG2P.e2m` in `misaki/espeak.py`.
const TIED: &[(&str, &str)] = &[
    ("a^ɪ", "I"),
    ("a^ʊ", "W"),
    ("d^z", "ʣ"),
    ("d^ʒ", "ʤ"),
    ("e^ɪ", "A"),
    ("o^ʊ", "O"),
    ("ə^ʊ", "Q"),
    ("s^s", "S"),
    ("t^s", "ʦ"),
    ("t^ʃ", "ʧ"),
    ("ɔ^ɪ", "Y"),
];

/// Spanish, French, Hindi, Italian, Brazilian Portuguese.
///
/// Deliberately leaves the combining tilde (U+0303) alone: nasal vowels are
/// real tokens in the model's vocabulary and dropping them, as the English
/// path does, would flatten `bɔ̃ʒˈuʁ` to `bɔʒˈuʁ`.
fn rewrite_romance(ps: &str) -> String {
    rewrite(ps, TIED).replace([TIE, '-'], "")
}

/// American and British English.
///
/// Port of `EspeakFallback` in `misaki/espeak.py`. misaki only reaches for
/// this when its English lexicon misses a word; we use it for the whole
/// utterance, so the phoneme alphabet is right but stress and vowel reduction
/// are eSpeak's rather than the lexicon's.
fn rewrite_english(ps: &str, british: bool) -> String {
    let mut rules: Vec<(&str, &str)> = vec![("ʔˌn\u{329}", "ʔn"), ("ʔn\u{329}", "ʔn")];
    // Dialect-specific tied vowels go in ahead of the bare-letter rules below.
    if british {
        rules.push(("e^ə", "ɛː"));
        rules.push(("ə^ʊ", "Q"));
    } else {
        rules.push(("o^ʊ", "O"));
    }
    rules.extend_from_slice(&[
        ("a^ɪ", "I"),
        ("a^ʊ", "W"),
        ("d^ʒ", "ʤ"),
        ("e^ɪ", "A"),
        ("t^ʃ", "ʧ"),
        ("ɔ^ɪ", "Y"),
        ("ə^l", "ᵊl"),
        ("ʲo", "jo"),
        ("ʲə", "jə"),
        ("ʲ", ""),
        ("ɚ", "əɹ"),
        ("ɐ", "ə"),
        ("ɬ", "l"),
        ("ç", "k"),
        ("x", "k"),
        ("r", "ɹ"),
        ("e", "A"),
        ("\u{303}", ""),
    ]);
    let mut ps = rewrite(ps, &rules);

    // A syllabic mark turns the consonant it sits under into "schwa + consonant".
    let mut syllabic = String::with_capacity(ps.len());
    for c in ps.chars() {
        if c == '\u{329}' {
            match syllabic.pop() {
                Some(prev) if !prev.is_whitespace() => {
                    syllabic.push('ᵊ');
                    syllabic.push(prev);
                }
                Some(prev) => syllabic.push(prev),
                None => {}
            }
        } else {
            syllabic.push(c);
        }
    }
    ps = syllabic;

    ps = if british {
        ps.replace("iə", "ɪə")
    } else {
        ps.replace("ɜːɹ", "ɜɹ")
            .replace("ɜː", "ɜɹ")
            .replace("ɪə", "iə")
            .replace('ː', "")
    };
    ps.replace('o', "ɔ")
        .replace('ɾ', "T")
        .replace('ʔ', "t")
        .replace(TIE, "")
}

/// Whether a `.` or `,` at `i` sits between two digits, i.e. it is a decimal
/// point or a thousands separator rather than punctuation.
///
/// Splitting there would hand eSpeak the halves separately, and "1,234"
/// becomes "one, two hundred thirty four" instead of "one thousand two
/// hundred and thirty four".
fn is_inside_number(chars: &[char], i: usize) -> bool {
    matches!(chars.get(i), Some('.') | Some(','))
        && i > 0
        && chars[i - 1].is_ascii_digit()
        && chars.get(i + 1).is_some_and(char::is_ascii_digit)
}

/// Punctuation the model has tokens for, and what we normalize it to.
fn punctuation_token(c: char) -> Option<char> {
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
        _ => return None,
    })
}

/// Text -> Kokoro phonemes, preserving punctuation across clause boundaries.
pub(crate) fn phonemize(
    text: &str,
    voices: &[&'static str],
    lang: Lang,
) -> Result<String, String> {
    let british = lang == Lang::BritishEnglish;
    let english = matches!(lang, Lang::AmericanEnglish | Lang::BritishEnglish);

    let mut out = String::new();
    let mut run = String::new();

    let flush = |run: &mut String, out: &mut String| -> Result<(), String> {
        if run.trim().is_empty() {
            run.clear();
            return Ok(());
        }
        let ps = phonemize_run(run.trim(), voices)?;
        let ps = if english {
            rewrite_english(&ps, british)
        } else {
            rewrite_romance(&ps)
        };
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
    fn rewrites_tied_diphthongs_to_single_tokens() {
        // What espeak en-us gives for "I made a mistake today", tie-marked.
        let ps = "a^ɪ mˌe^ɪd ɐ mɪstˈe^ɪk tədˈe^ɪ";
        let got = rewrite_english(ps, false);
        assert!(got.contains('I'), "aɪ should collapse to I: {got}");
        assert_eq!(got.matches('A').count(), 3, "three /eɪ/ in {got}");
        assert!(!got.contains('^'), "ties must not survive: {got}");
        assert!(!got.contains('ɐ'), "ɐ folds onto ə: {got}");
    }

    #[test]
    fn british_and_american_diverge_on_the_goat_vowel() {
        assert!(rewrite_english("ɡˌə^ʊ", true).contains('Q'));
        assert!(rewrite_english("ɡˌo^ʊ", false).contains('O'));
    }

    #[test]
    fn british_keeps_length_marks_american_drops_them() {
        assert!(rewrite_english("ɑːɹ", true).contains('ː'));
        assert!(!rewrite_english("ɑːɹ", false).contains('ː'));
    }

    #[test]
    fn tie_bearing_rules_win_over_bare_letter_rules() {
        // 'e' -> 'A' must not fire first and strand the British `eə`.
        assert_eq!(rewrite_english("ðˈe^ə", true), "ðˈɛː");
    }

    #[test]
    fn romance_keeps_nasal_vowels() {
        let got = rewrite_romance("bɔ\u{303}ʒˈuʁ");
        assert!(
            got.contains('\u{303}'),
            "nasalization is a real token: {got}"
        );
    }

    #[test]
    fn romance_does_not_apply_english_only_rewrites() {
        // Spanish taps and rhotics must survive; only English folds ɾ onto T.
        assert!(rewrite_romance("pˈaɾke").contains('ɾ'));
        assert!(rewrite_english("pˈaɾke", false).contains('T'));
    }

    #[test]
    fn a_system_install_is_found_without_any_environment_variable() {
        // The regression this guards: with no env var set we used to hand
        // espeak a null path, and it fell back to the OUT_DIR baked into the
        // library at build time - a `target/` path that is routinely stale.
        for var in DATA_DIR_ENVS {
            if std::env::var_os(var).is_some() {
                return; // an explicit path is configured; nothing to prove
            }
        }
        assert!(
            data_dir().is_some(),
            "no espeak-ng-data found in any of {:?}",
            data_dir_candidates()
        );
    }

    #[test]
    fn candidates_are_ordered_and_deduplicated() {
        let candidates = data_dir_candidates();
        let mut seen = std::collections::HashSet::new();
        for c in &candidates {
            assert!(seen.insert(c.clone()), "duplicate candidate {c:?}");
        }
        assert!(candidates.iter().any(|c| c.starts_with("/usr")));
    }

    #[test]
    fn number_separators_are_not_clause_breaks() {
        let chars: Vec<char> = "1,234.5 and, then.".chars().collect();
        assert!(is_inside_number(&chars, 1), "thousands separator");
        assert!(is_inside_number(&chars, 5), "decimal point");
        assert!(!is_inside_number(&chars, 11), "a real comma");
        assert!(!is_inside_number(&chars, 17), "a real full stop");
    }

    #[test]
    fn language_switch_flags_are_removed() {
        // espeak falls back to another language's rules for foreign words and
        // says so inline; those markers are not phonemes.
        assert_eq!(strip_language_flags("a(en)bc(hi)d"), "abcd");
        assert_eq!(strip_language_flags("no flags here"), "no flags here");
    }

    #[test]
    fn syllabic_marks_become_leading_schwa() {
        assert_eq!(rewrite_english("bˈʌtn\u{329}", false), "bˈʌtᵊn");
    }
}
