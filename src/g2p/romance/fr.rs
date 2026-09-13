//! French grapheme-to-phoneme.
//!
//! French is the least regular of the four Romance languages here: a large
//! share of its irregularity is silent letters and nasal vowels, both of
//! which are still rule-governed (unlike English's lexical irregularity),
//! just by rules with more moving parts. What's implemented:
//!
//!   * Nasal vowels: a vowel followed by `n`/`m` that is *not* itself
//!     followed by a vowel or another `n`/`m` nasalizes and the `n`/`m`
//!     itself is silent ("bon" -> /bɔ̃/, but "bonne" -> /bɔn/, "banane" ->
//!     oral throughout).
//!   * Word-final consonants are silent unless they are one of `c r f l`
//!     ("CaReFuL"), applied repeatedly since dropping one can expose
//!     another ("chats" drops the plural s, then "chat"'s own silent t:
//!     /ʃa/, not /ʃat/). `-er` is its own exception (infinitives and the
//!     common `-er` noun/adjective suffix are /e/ with a silent r -
//!     "parler", "danger", not "parlerr"/"dangerr") - except a lexicon of
//!     common words that keep the r regardless ("cher", "fer", "hiver",
//!     ... - a `KEEPS_R` exception list inside `phonemize_word`).
//!   * A word-final unaccented `e` is silent, whatever precedes it - a
//!     consonant ("petite", not silencing the `t` before it, which is the
//!     point of writing it), another vowel ("vie", "pharmacie" end in the
//!     vowel before the e, not /iə/), or "que"/"gue" (the `u` there is a
//!     silent hardness marker for q/g, not a real vowel - "musique" ends in
//!     /ik/, not /ikə/). A bare `e` that survives - mid-word, or word-final
//!     behind a CaReFuL consonant - is /ɛ/ rather than schwa when its
//!     syllable is closed (followed by two consonant letters, even a
//!     doubled one that collapses to a single sound, or by exactly one that
//!     ends the word): "avec", "chef", "elle", "reste" are /ɛ/, not /ə/.
//!     Not attempted: the "ex-" prefix has its own fixed /ɛgz/ regardless
//!     of context, which this general rule does not special-case.
//!   * A single `s` between two vowel sounds voices to /z/ ("maison",
//!     "chemise"); a doubled `ss` never does ("chasse", "poisson"). A short
//!     lexicon of compounds whose second element is a recognizable
//!     standalone French word starting with "s" keeps it unvoiced instead
//!     ("parasol", "tournesol" - see `NO_S_VOICING`), since speakers treat
//!     that "s" as word-initial within the compound.
//!   * `ph` is /f/; a doubled `r` collapses to one /ʁ/ the same way every
//!     other doubled consonant does ("guerre" is /ɡɛʁ/, one r, not two).
//!   * "est" (the copula) and its elided compounds ("c'est", "n'est",
//!     "qu'est", "s'est") are /ɛ/ - both the s and the t are silent, unlike
//!     a generic word ending in "-est" (compare "ouest", "test", "lest",
//!     which keep both). This also mis-reads the much rarer noun "est"
//!     ("east"), which does pronounce the t - an accepted trade-off given
//!     how much more common the copula is in ordinary text.
//!   * Liaison: a small set of very common, unambiguous trigger words
//!     (plural articles/determiners, subject pronouns, "est" and a few
//!     adjectives - see [`LIAISON`]) resurface their normally-silent final
//!     consonant when the next word starts with a vowel sound. "Vowel
//!     sound" accounts for "h aspiré" ([`H_ASPIRE`], a closed list - nothing
//!     in the spelling marks these) and the handful of words that behave as
//!     consonant-initial despite their spelling ([`NO_LIAISON_VOWEL`]:
//!     "onze", "oui", ...). Deliberately excluded: trigger words whose
//!     liaison consonant is a resurfacing nasal ("un", "on", "bon", "mon",
//!     "ton", "son", "en", "bien", "sont", "non", ...), since those need the
//!     vowel to switch from nasalized back to oral at the same moment the
//!     consonant reappears, which is more machinery than this first pass
//!     attempts; and noun-adjective and adjective-noun liaison generally,
//!     which is far more lexically variable than the closed classes here.
//!   * Stress is always the last pronounced syllable - true nucleus-level
//!     French stress is a property of the phrase, not the word, but
//!     word-at-a-time is what this crate does throughout.

use super::is_unstressed_function_word;
use crate::g2p::text;

/// Articles, simple (including elided) prepositions, coordinating
/// conjunctions and clitic pronouns: none of these carry a stress mark,
/// even mid-sentence ("me", "vu", "dœ̃"/"un", "kə"/"que", "lə"/"le" are all
/// unstressed).
const UNSTRESSED: &[&str] = &[
    "le", "la", "les", "un", "une", "des", "de", "du", "à", "en", "et", "ou", "que", "qui", "se",
    "ce", "ces", "ne", "me", "te", "vous", "nous", "mon", "ma", "mes", "ton", "ta", "tes", "son",
    "sa", "ses", "est", "je", "tu", "il", "ils", "elle", "elles", "on",
];

/// A handful of very common function words that are the copula "être" (or
/// an apostrophe-elided pronoun plus it) rather than a "-est"-ending content
/// word: "est"/"c'est"/"n'est"/"qu'est"/"s'est" are /ɛ/ with a silent s and
/// t, unlike "ouest"/"test"/"lest" which keep both. Matched on the whole
/// word (apostrophe included) precisely so those other, unrelated "-est"
/// words are never touched.
const EST_COPULA: &[(&str, &str)] = &[
    ("est", ""),
    ("c'est", "s"),
    ("n'est", "n"),
    ("qu'est", "k"),
    ("s'est", "s"),
];

/// Obligatory-liaison trigger words, mapped to the consonant that surfaces
/// when the next word starts with a vowel sound. See the module docs for
/// what this deliberately excludes.
const LIAISON: &[(&str, &str)] = &[
    ("les", "z"), ("des", "z"), ("mes", "z"), ("tes", "z"), ("ses", "z"), ("ces", "z"),
    ("aux", "z"), ("ils", "z"), ("elles", "z"), ("nous", "z"), ("vous", "z"),
    ("quels", "z"), ("quelles", "z"), ("plusieurs", "z"), ("certains", "z"), ("certaines", "z"),
    ("deux", "z"), ("trois", "z"), ("gros", "z"), ("grosse", "z"),
    ("petit", "t"), ("petits", "z"), ("grand", "t"), ("grands", "z"), ("tout", "t"),
    ("est", "t"), ("c'est", "t"), ("n'est", "t"), ("qu'est", "t"), ("s'est", "t"),
];

/// Cardinal numbers whose final consonant does something plain
/// liaison-consonant-append (see [`LIAISON`]) can't express: it is present,
/// absent, or (for "six"/"dix") a *different* consonant, depending on
/// whether the next word starts with a consonant, is absent (phrase-final),
/// or starts with a vowel sound. Each entry is `(word, standalone form,
/// form before a consonant, form before a vowel sound)`. "sept" doesn't
/// actually vary by context - it is here anyway with the same form
/// repeated three times, because its base pipeline output is otherwise
/// wrong (see the "sept" entry itself for why).
const CONTEXTUAL_NUMBER: &[(&str, &str, &str, &str)] = &[
    ("six", "s\u{2c8}is", "s\u{2c8}i", "s\u{2c8}iz"),
    ("dix", "d\u{2c8}is", "d\u{2c8}i", "d\u{2c8}iz"),
    ("huit", "\u{265}\u{2c8}it", "\u{265}\u{2c8}i", "\u{265}\u{2c8}it"),
    ("vingt", "v\u{2c8}\u{25b}\u{303}", "v\u{2c8}\u{25b}\u{303}", "v\u{2c8}\u{25b}\u{303}t"),
    // "sept" does not vary by context at all (unlike the four above) - it
    // is simply routed through this same table with the same form in all
    // three slots, since its base pipeline output is otherwise wrong (the
    // p+t cluster falls afoul of the general final-consonant-cascade rule,
    // which drops both when only the p should ever be silent).
    ("sept", "s\u{2c8}\u{25b}t", "s\u{2c8}\u{25b}t", "s\u{2c8}\u{25b}t"),
];

/// Words starting with an "h aspiré" - orthographically silent like any
/// other French h, but still blocking liaison/elision the way a real
/// consonant would ("les héros" is /le.eʁo/, not /lez‿eʁo/). Not
/// exhaustive - there is no way to derive this from spelling, only to list
/// known cases.
const H_ASPIRE: &[&str] = &[
    "hache", "haie", "haine", "haïr", "hall", "halte", "hamac", "hanche", "handicap", "hangar",
    "harceler", "hargne", "haricot", "harpe", "hasard", "hâte", "hausse", "haut", "hauteur",
    "hérisson", "héros", "hêtre", "hibou", "hockey", "homard", "honte", "hors", "houle", "housse",
    "huit", "hurler", "hutte",
];

/// Words that behave as if they start with a consonant despite their
/// spelling, blocking both liaison and elision ("le onze", not "l'onze";
/// "les onze joueurs" has no liaison z).
const NO_LIAISON_VOWEL: &[&str] = &["onze", "oui", "ouistiti", "uhlan", "yaourt", "yacht"];

const CAREFUL: &[char] = &['c', 'r', 'f', 'l'];

/// Compounds where the second element is a recognizable standalone French
/// word starting with "s" ("sol", "semblable", "social"): speakers treat
/// that "s" as word-initial within the compound rather than truly
/// intervocalic, so it stays /s/ where the general rule below would
/// otherwise voice it to /z/. Not exhaustive, since there is no way to
/// derive "is this a compound with a felt internal boundary" from spelling
/// alone.
const NO_S_VOICING: &[&str] = &[
    "parasol", "parasols", "tournesol", "tournesols", "antisocial", "antisociale",
    "antisociaux", "antisociales", "vraisemblable", "vraisemblables",
];

fn is_vowel(c: char) -> bool {
    matches!(c, 'a' | 'e' | 'i' | 'o' | 'u' | 'y' | 'é' | 'è' | 'ê' | 'ë' | 'à' | 'â' | 'ô' | 'û' | 'ù' | 'î' | 'ï' | 'œ')
}

/// Whether the letter at `i` is a nasalizing `n`/`m`: consonant is silent,
/// the preceding vowel(s) become nasal.
fn nasalizes(letters: &[char], i: usize) -> bool {
    matches!(letters[i], 'n' | 'm')
        && !letters.get(i + 1).copied().is_some_and(|c| is_vowel(c) || matches!(c, 'n' | 'm'))
}

fn phonemize_word(word: &str) -> String {
    let word_lower = word.to_lowercase();
    if let Some((_, prefix)) = EST_COPULA.iter().find(|(k, _)| *k == word_lower) {
        // Bypasses the general pipeline entirely: matched on the whole word
        // precisely so this never touches "ouest"/"test"/"lest".
        return format!("{prefix}\u{25b}");
    }

    let mut letters: Vec<char> = word.to_lowercase().chars().filter(|c| c.is_alphabetic()).collect();
    if letters.is_empty() {
        return String::new();
    }
    let n0 = letters.len();
    let word_lower_for_er_check = letters.iter().collect::<String>();

    // "-er" infinitives and the same-shaped noun/adjective ending (danger,
    // boulanger, escalier, léger, premier, ...) are silent-r, vowel /e/ -
    // but a smallish set of common words spelled the same way keep the r
    // with an open /ɛ/ instead ("cher", "fer", "hiver", ...). There is no
    // way to tell these apart from spelling alone; this is a lexicon of
    // known exceptions, not remotely exhaustive.
    const KEEPS_R: &[&str] = &[
        "cher", "chere", "fer", "mer", "hier", "fier", "fiere", "hiver", "super", "cancer",
        "enfer", "ver", "revolver", "cuiller", "ether",
    ];
    let er_infinitive = n0 > 2
        && letters[n0 - 2] == 'e'
        && letters[n0 - 1] == 'r'
        && !KEEPS_R.contains(&word_lower_for_er_check.as_str());
    if er_infinitive {
        letters.pop();
    }

    // "les"/"mes"/"ces"/"des"/"ses"/"tes": these spell /e/, not the general
    // "-es is silent" pattern below - they are indivisible monosyllables,
    // not a stem plus a plural/verb "-es" ending. Same treatment as the
    // "-er" case: drop the trailing s, render the surviving e as /e/.
    let es_function_word = !er_infinitive
        && n0 == 3
        && letters[1] == 'e'
        && letters[2] == 's'
        && matches!(letters[0], 'l' | 'm' | 'c' | 'd' | 's' | 't');
    if es_function_word {
        letters.pop();
    }
    let renders_final_e_as_e = er_infinitive || es_function_word;

    // Whether the letter that ends up last (after all the truncation below)
    // was, in the original spelling, immediately followed by a vowel that
    // this function is about to drop (a mute final e). "chemise" truncates
    // to "chemis" - its final s needs to know a vowel *used* to follow it
    // to voice to /z/ correctly, since by the time the main loop below sees
    // it, that vowel is gone from the array entirely.
    let mut final_consonant_was_prevocalic = false;

    let word_len = if renders_final_e_as_e {
        letters.len()
    } else {
        let n = letters.len();

        // The general "-es" ending (regular plural nouns/adjectives, 2nd
        // person singular verbs: "portes", "tables", "tu parles") is
        // entirely silent - both the e and the s - unlike the closed-class
        // function words above.
        let general_es_ending = n > 2
            && letters[n - 1] == 's'
            && letters[n - 2] == 'e'
            && !is_vowel(letters[n - 3])
            && letters[..n - 2].iter().any(|&c| is_vowel(c));
        if general_es_ending {
            n - 2
        } else {
            // Trailing unaccented mute "e" after a consonant: drop it, but
            // leave the consonant before it (which is what makes it
            // pronounced) untouched.
            //
            // Not when it is the word's only vowel: monosyllables like
            // "je", "le", "de", "ce", "se", "ne", "me", "te" need that e as
            // their syllable nucleus - dropping it would leave no vowel at
            // all, not a shorter but still pronounceable word.
            // A word-final unaccented e is mute regardless of what
            // precedes it - not just after a consonant ("petite") but
            // after a vowel too ("vie", "pharmacie", "année" all end in
            // the preceding vowel's sound, not /iə/), and after "que"/"gue"
            // (the u there is a silent hardness marker for q/g, not a real
            // vowel - "musique"/"langue" end in /ik/, /ɑ̃ɡ/). The only
            // guard needed is the existing "don't strip a monosyllable's
            // only vowel" one, which a vowel-preceded e never triggers
            // anyway (the vowel right before it already satisfies it).
            let mute_e = n > 1
                && letters[n - 1] == 'e'
                && letters[..n - 1].iter().any(|&c| is_vowel(c));
            final_consonant_was_prevocalic = mute_e;
            let word_len = if mute_e { n - 1 } else { n };

            // Final consonant silencing (CaReFuL), applied repeatedly: not
            // just once. Dropping one silent consonant can expose another
            // that is also silent - "chats" drops the plural s, exposing
            // "chat"'s own silent t ("les chats" is /le ʃa/, not /le ʃat/,
            // same as "plats" -> /pla/). Not n/m: those are consumed by the
            // nasal-vowel rule in the main loop below, not by silent
            // deletion here - dropping them first would hide them from that
            // rule.
            let mut word_len = word_len;
            if !mute_e {
                while word_len > 1
                    && !is_vowel(letters[word_len - 1])
                    && !matches!(letters[word_len - 1], 'n' | 'm')
                    && !CAREFUL.contains(&letters[word_len - 1])
                {
                    word_len -= 1;
                }
            }
            word_len
        }
    };

    let letters = &letters[..word_len];
    let n = letters.len();

    let mut out = String::new();
    let mut i = 0;
    while i < n {
        let c = letters[i];
        let next = letters.get(i + 1).copied();
        let next2 = letters.get(i + 2).copied();

        // The infinitive/noun "-er" ending's and les/mes/ces/des/ses/tes's
        // surviving e is /e/, not the default schwa a bare word-medial "e"
        // gets.
        if renders_final_e_as_e && i == n - 1 && c == 'e' {
            out.push('e');
            i += 1;
            continue;
        }

        // Nasal vowels, longest pattern first.
        if matches!(c, 'a' | 'e') && next == Some('i') && next2.is_some_and(|c2| nasalizes(letters, i + 2)) {
            out.push('\u{25b}'); // ɛ
            out.push('\u{303}');
            i += 3;
            continue;
        }
        if c == 'i' && next == Some('e') && next2 == Some('n') && nasalizes(letters, i + 2) {
            out.push('j');
            out.push('\u{25b}');
            out.push('\u{303}');
            i += 3;
            continue;
        }
        if matches!(c, 'a' | 'e') && next.is_some_and(|nx| nasalizes(letters, i + 1) && matches!(nx, 'n' | 'm')) {
            out.push('\u{251}'); // ɑ
            out.push('\u{303}');
            i += 2;
            continue;
        }
        if c == 'i' && next.is_some_and(|nx| nasalizes(letters, i + 1) && matches!(nx, 'n' | 'm')) {
            out.push('\u{25b}'); // ɛ̃ - "in"/"im"
            out.push('\u{303}');
            i += 2;
            continue;
        }
        if c == 'o' && next.is_some_and(|nx| nasalizes(letters, i + 1) && matches!(nx, 'n' | 'm')) {
            out.push('\u{254}'); // ɔ
            out.push('\u{303}');
            i += 2;
            continue;
        }
        if c == 'u' && next.is_some_and(|nx| nasalizes(letters, i + 1) && matches!(nx, 'n' | 'm')) {
            out.push('\u{153}'); // œ
            out.push('\u{303}');
            i += 2;
            continue;
        }

        match c {
            'e' if next == Some('a') && next2 == Some('u') => {
                out.push('o');
                i += 3;
            }
            'a' | 'e' if next == Some('u') => {
                out.push('o');
                i += 2;
            }
            'o' if next == Some('u') => {
                out.push('u');
                i += 2;
            }
            'o' if next == Some('i') => {
                out.push_str("wa");
                i += 2;
            }
            'e' if next == Some('u') => {
                out.push('\u{f8}'); // ø
                i += 2;
            }
            'a' if next == Some('i') => {
                out.push('\u{25b}'); // ɛ
                i += 2;
            }
            'e' if next == Some('i') => {
                out.push('\u{25b}');
                i += 2;
            }
            'g' if next == Some('n') => {
                out.push('\u{272}'); // ɲ
                i += 2;
            }
            // "gu" before e/i, or before nothing when a trailing e was
            // already truncated away here as mute ("langue"/"vague"), is a
            // silent-u hard /ɡ/ - same shape as "qu" below, just for g.
            // "aigu" (real word-final u, actually pronounced) is
            // unaffected: it never goes through the mute-e truncation that
            // sets `final_consonant_was_prevocalic`.
            'g' if next == Some('u')
                && (matches!(next2, Some('e') | Some('i'))
                    || (next2.is_none() && final_consonant_was_prevocalic)) =>
            {
                out.push('\u{261}'); // ɡ
                i += 2;
            }
            'c' if next == Some('h') => {
                out.push('\u{283}'); // ʃ
                i += 2;
            }
            'q' if next == Some('u') => {
                out.push('k');
                i += 2;
            }
            'c' if matches!(next, Some('e') | Some('i') | Some('y')) => {
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
            'g' if matches!(next, Some('e') | Some('i') | Some('y')) => {
                out.push('\u{292}'); // ʒ
                i += 1;
            }
            'g' => {
                out.push('\u{261}'); // ɡ
                i += 1;
            }
            'h' => {
                i += 1; // always silent
            }
            'p' if next == Some('h') => {
                out.push('f');
                i += 2;
            }
            'r' => {
                out.push('\u{281}'); // ʁ, uvular
                // Same double-letter collapse as the general consonant arm
                // below: "guerre"/"terre"/"pierre" are /ɛʁ/, one r sound,
                // not two.
                i += if next == Some('r') { 2 } else { 1 };
            }
            'j' => {
                out.push('\u{292}'); // ʒ
                i += 1;
            }
            'y' => {
                out.push('j');
                i += 1;
            }
            'u' => {
                out.push('y'); // front rounded /y/, ascii 'y' used for it here
                i += 1;
            }
            'œ' => {
                out.push('\u{f8}');
                i += 1;
            }
            'x' => {
                out.push_str("ks");
                i += 1;
            }
            'é' => {
                out.push('e');
                i += 1;
            }
            'è' | 'ê' | 'ë' => {
                out.push('\u{25b}');
                i += 1;
            }
            'à' | 'â' | 'a' => {
                out.push('a');
                i += 1;
            }
            'ô' => {
                out.push('o');
                i += 1;
            }
            'î' | 'ï' | 'i' => {
                out.push('i');
                i += 1;
            }
            'e' => {
                // "Loi de position": a bare e is /ɛ/, not schwa, when its
                // syllable is closed - followed by two or more consonant
                // letters (even a doubled one that collapses to a single
                // sound, e.g. "cette"/"elle": the orthographic doubling is
                // itself the closed-syllable signal), or by exactly one
                // consonant that ends the word ("chef", "avec", "sel").
                // One known gap: the "ex-" prefix has its own fixed /ɛgz/
                // regardless of context, not attempted here.
                let mut j = i + 1;
                let mut consonants_ahead = 0u32;
                while j < n && !is_vowel(letters[j]) {
                    consonants_ahead += 1;
                    j += 1;
                }
                let closed_syllable = consonants_ahead >= 2 || (consonants_ahead == 1 && j == n);
                out.push(if closed_syllable { '\u{25b}' } else { '\u{259}' });
                i += 1;
            }
            'o' => {
                out.push('o');
                i += 1;
            }
            // A single "s" between two vowel sounds voices to /z/ ("maison",
            // "chemise" - including when the following vowel is a mute
            // final e this function already truncated away, tracked via
            // `final_consonant_was_prevocalic`), except inside a handful of
            // compounds where the second element keeps its own word-initial
            // /s/ (see `NO_S_VOICING`). A doubled "ss" never matches this
            // arm at all: the general collapsing rule reads it as one plain
            // /s/ two letters later, same as any other double letter.
            's' if i > 0
                && is_vowel(letters[i - 1])
                && next != Some('s')
                && (next.map(is_vowel).unwrap_or(false)
                    || (next.is_none() && final_consonant_was_prevocalic))
                && !NO_S_VOICING.contains(&word_lower.as_str()) =>
            {
                out.push('z');
                i += 1;
            }
            'b' | 'd' | 'f' | 'k' | 'l' | 'm' | 'n' | 'p' | 's' | 't' | 'v' | 'w' | 'z' => {
                out.push(c);
                // Double letters are not geminate in French (unlike Italian):
                // "allez" is /ale/, "comment" is /kɔmɑ̃/ - one consonant, not
                // two - so a doubled letter (once it has been ruled out as an
                // n/m denasalizer above) collapses to a single phoneme.
                i += if next == Some(c) { 2 } else { 1 };
            }
            _ => {
                i += 1;
            }
        }
    }

    // Stress on the last pronounced syllable: French does not vary stress by
    // word shape the way Spanish/Italian do, so no accent-based override is
    // needed, only "find the last vowel and mark it" - except for articles,
    // prepositions, conjunctions and clitic pronouns, which stay unstressed.
    if !is_unstressed_function_word(word, UNSTRESSED) {
        if let Some(pos) = find_last_stressable_vowel(&out) {
            out.insert(pos, '\u{2c8}');
        }
    }
    out
}

/// Byte offset just before the character that starts the last vowel (a
/// contiguous run of vowel/nasal-marker characters) in the phonemized
/// output, so the stress mark lands before the syllable rather than
/// mid-vowel.
fn find_last_stressable_vowel(ps: &str) -> Option<usize> {
    let chars: Vec<char> = ps.chars().collect();
    let is_ph_vowel = |c: char| {
        matches!(
            c,
            'a' | 'e' | 'i' | 'o' | 'u' | 'y' | '\u{259}' | '\u{25b}' | '\u{254}' | '\u{251}'
                | '\u{f8}' | '\u{153}'
        )
    };
    let last = chars.iter().rposition(|&c| is_ph_vowel(c))?;
    let mut start = last;
    while start > 0 && is_ph_vowel(chars[start - 1]) {
        start -= 1;
    }
    Some(chars[..start].iter().map(|c| c.len_utf8()).sum())
}

/// Whether `next_word` (as written, punctuation already stripped by the
/// clause splitter) starts with a vowel *sound* - true for a written vowel,
/// for a silent ("muet") h, but not for an "h aspiré" word or one of the
/// small set of words that behave as consonant-initial regardless of
/// spelling ("onze", "oui", ...).
fn starts_with_vowel_sound(next_word: &str) -> bool {
    let w: String = next_word
        .chars()
        .filter(|c| c.is_alphabetic())
        .collect::<String>()
        .to_lowercase();
    if NO_LIAISON_VOWEL.contains(&w.as_str()) {
        return false;
    }
    match w.chars().next() {
        Some(c) if "aeiouyàâéèêëîïôûùœ".contains(c) => true,
        Some('h') => !H_ASPIRE.iter().any(|h| w.starts_with(h)),
        _ => false,
    }
}

/// The liaison consonant `word` surfaces when the next word starts with a
/// vowel sound, if `word` is one of the (deliberately small - see the
/// module docs) set of unambiguous obligatory-liaison triggers this handles.
fn liaison_consonant(word: &str) -> Option<&'static str> {
    let w = word.to_lowercase();
    LIAISON.iter().find(|(k, _)| *k == w).map(|(_, v)| *v)
}

pub(crate) fn phonemize(text_in: &str) -> Result<String, String> {
    text::phonemize_clauses(text_in, |run| {
        let words: Vec<&str> = run.split_whitespace().collect();
        let mut out: Vec<String> = Vec::with_capacity(words.len());
        for (i, &w) in words.iter().enumerate() {
            let w_lower = w.to_lowercase();
            if let Some(&(_, standalone, before_consonant, before_vowel)) =
                CONTEXTUAL_NUMBER.iter().find(|(k, ..)| *k == w_lower)
            {
                let form = match words.get(i + 1) {
                    None => standalone,
                    Some(&next) if starts_with_vowel_sound(next) => before_vowel,
                    Some(_) => before_consonant,
                };
                out.push(form.to_string());
                continue;
            }
            let mut ps = phonemize_word(w);
            if let Some(consonant) = liaison_consonant(w) {
                if words.get(i + 1).is_some_and(|&next| starts_with_vowel_sound(next)) {
                    ps.push_str(consonant);
                }
            }
            out.push(ps);
        }
        Ok(out.join(" "))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nasal_vowel_before_word_final_n() {
        let ps = phonemize_word("bon");
        assert!(ps.contains('\u{303}'), "{ps} should be nasal");
        assert!(!ps.contains('n'), "the n itself is silent: {ps}");
    }

    #[test]
    fn n_stays_oral_and_pronounced_before_a_vowel() {
        let ps = phonemize_word("bonne");
        assert!(!ps.contains('\u{303}'), "{ps} should not be nasal");
        assert!(ps.contains('n'), "{ps} should keep the consonant");
    }

    #[test]
    fn er_infinitive_drops_the_r() {
        // "parler" has two r's: the stem's (pronounced, /paʁl.../) and the
        // "-er" ending's (silent) - only the trailing one should disappear.
        let ps = phonemize_word("parler");
        assert!(ps.ends_with('e'), "-er infinitives end in /e/, not /ɛʁ/: {ps}");
    }

    #[test]
    fn careful_consonants_survive_word_final() {
        assert!(phonemize_word("chef").contains('f'));
        assert!(phonemize_word("bar").contains('\u{281}'));
    }

    #[test]
    fn non_careful_final_consonants_are_silent() {
        let ps = phonemize_word("petit");
        assert!(!ps.ends_with('t'), "{ps}: word-final t is silent");
    }

    #[test]
    fn mute_e_reactivates_the_preceding_consonant() {
        let ps = phonemize_word("petite");
        assert!(ps.ends_with('t'), "{ps}: e reactivates the t, then is dropped itself");
    }

    #[test]
    fn a_word_final_e_that_is_the_only_vowel_is_not_dropped() {
        // "je", "le", "de", "ce", "se", "ne", "me", "te": a bare
        // consonant+e monosyllable needs that e as its one syllable
        // nucleus - unlike "petite", there is no other vowel to fall back
        // on, so it must survive as a schwa rather than be treated as a
        // silent trailing e.
        for word in ["je", "le", "de", "ce", "se", "ne", "me", "te"] {
            let ps = phonemize_word(word);
            assert!(!ps.is_empty(), "{word} phonemized to nothing");
            assert!(
                ps.chars().any(|c| "aeiouyœø\u{259}\u{25b}\u{254}\u{251}".contains(c)),
                "{word} -> {ps:?}: lost its only vowel"
            );
        }
    }

    #[test]
    fn les_mes_ces_des_ses_tes_render_e_not_schwa() {
        // Confirmed against actual French pronunciation: these six spell
        // /e/ (the closed vowel), distinct from singular "le"/"ce"/"se"
        // which are schwa - unlike a regular "-es" ending, they are not
        // decomposed as stem + silent suffix. All six are also unstressed
        // function words (see `function_words_are_unstressed`), so no
        // stress mark either.
        for (word, consonant) in [
            ("les", 'l'),
            ("mes", 'm'),
            ("ces", 's'),
            ("des", 'd'),
            ("ses", 's'),
            ("tes", 't'),
        ] {
            let ps = phonemize_word(word);
            assert_eq!(ps, format!("{consonant}e"), "{word} -> {ps:?}");
        }
    }

    #[test]
    fn regular_plural_and_verb_es_endings_are_fully_silent() {
        // "portes" ([pɔʁt]) and "tu parles" ([paʁl]): both the e and the s
        // vanish, unlike les/mes/ces/des/ses/tes above.
        const VOWELS: &str = "aeiouyœø\u{259}\u{25b}\u{254}\u{251}";
        for word in ["portes", "parles", "tables"] {
            let ps = phonemize_word(word);
            let last = ps.chars().last().expect("non-empty");
            assert!(
                !VOWELS.contains(last),
                "{word} -> {ps:?}: the -es ending should be fully silent"
            );
        }
    }

    #[test]
    fn function_words_are_unstressed() {
        for word in ["le", "de", "en", "que", "vous", "un"] {
            let ps = phonemize_word(word);
            assert!(!ps.contains('\u{2c8}'), "{word} -> {ps:?}: should be unstressed");
        }
    }

    #[test]
    fn est_copula_is_open_e_not_schwa_plus_s() {
        // "il est" is /il ɛ/, not /il əs/ - the s and t are both silent,
        // unlike the "ouest"/"test"/"lest" that share the "-est" spelling.
        assert_eq!(phonemize_word("est"), "\u{25b}");
        assert_eq!(phonemize_word("c'est"), "s\u{25b}");
        assert_eq!(phonemize_word("ouest"), phonemize_word("ouest")); // sanity: does not panic
        assert_ne!(phonemize_word("ouest"), "\u{25b}", "ouest keeps its consonants, unlike the copula");
    }

    #[test]
    fn liaison_adds_the_consonant_before_a_vowel_sound() {
        let ps = phonemize("les amis").unwrap();
        assert!(ps.contains("lez"), "{ps}: \"les\" should liaise /z/ before a vowel");
    }

    #[test]
    fn liaison_does_not_apply_before_a_consonant() {
        let ps = phonemize("les chats").unwrap();
        assert!(!ps.contains("lez"), "{ps}: no liaison before a consonant-initial word");
    }

    #[test]
    fn liaison_is_blocked_by_h_aspire() {
        // Matched with its real French spelling (accented é), not a plain-
        // ASCII stand-in: the accent is exactly what a naive "compare
        // without stripping diacritics first" bug would miss.
        let ps = phonemize("les héros").unwrap();
        assert!(!ps.contains("lez"), "{ps}: \"héros\" starts with h aspiré, liaison is blocked");
    }

    #[test]
    fn cascading_final_consonant_deletion() {
        // "chats": the plural "s" drops first, exposing "chat"'s own
        // silent "t" - both must go ("les chats" is /le ʃa/, not /le ʃat/).
        // A single-pass rule only removed the outer one.
        assert!(!phonemize_word("chats").ends_with('t'), "{:?}", phonemize_word("chats"));
        assert!(!phonemize_word("plats").ends_with('t'), "{:?}", phonemize_word("plats"));
        // Sanity: a lone silent consonant (no plural s) was already fine.
        assert!(!phonemize_word("chat").ends_with('t'));
    }

    #[test]
    fn liaison_applies_through_a_silent_h() {
        let ps = phonemize("les hommes").unwrap();
        assert!(ps.contains("lez"), "{ps}: \"hommes\" starts with h muet, liaison applies");
    }

    #[test]
    fn liaison_is_blocked_for_onze() {
        let ps = phonemize("les onze").unwrap();
        assert!(!ps.contains("lez"), "{ps}: \"onze\" behaves as consonant-initial");
    }

    #[test]
    fn est_liaises_its_t() {
        let ps = phonemize("il est arrive").unwrap();
        assert!(ps.contains('t'), "{ps}: \"est\" should liaise /t/ before a vowel-initial word");
    }

    #[test]
    fn closed_syllable_e_is_open_not_schwa() {
        // A bare e followed by two consonants, or by one that ends the
        // word, is /ɛ/ - not the default schwa a word-medial e otherwise
        // gets.
        for (word, expect_ipa) in [
            ("chef", '\u{25b}'),   // one consonant, word-final
            ("avec", '\u{25b}'),   // one consonant, word-final
            ("elle", '\u{25b}'),   // doubled consonant (single sound, still "closed")
            ("bref", '\u{25b}'),
            ("sel", '\u{25b}'),
            ("reste", '\u{25b}'),  // two distinct consonants
        ] {
            let ps = phonemize_word(word);
            assert!(ps.contains(expect_ipa), "{word} -> {ps:?}: expected {expect_ipa:?}");
            assert!(!ps.contains('\u{259}'), "{word} -> {ps:?}: should not also contain schwa");
        }
    }

    #[test]
    fn open_syllable_e_stays_schwa_or_deletes_normally() {
        // Sanity check the closed-syllable rule doesn't fire when it
        // shouldn't: a single consonant followed by another vowel is the
        // ordinary schwa-eligible (open syllable) case, unaffected by it.
        // "semaine": the first e is followed by a single consonant (m)
        // then another vowel (a) - the ordinary open-syllable case - so it
        // must stay schwa, not become closed-syllable /ɛ/.
        assert!(
            phonemize_word("semaine").starts_with("s\u{259}m"),
            "{:?}: open-syllable e should stay schwa",
            phonemize_word("semaine")
        );
    }

    #[test]
    fn common_er_words_keep_their_r_and_open_e() {
        // "cher"/"fer"/"hiver"/... are common exceptions to the otherwise
        // reliable "-er is silent-r" pattern ("danger", "parler" do drop
        // it).
        for (word, expect_ipa) in [("cher", '\u{25b}'), ("fer", '\u{25b}'), ("hiver", '\u{25b}')] {
            let ps = phonemize_word(word);
            assert!(ps.contains('\u{281}'), "{word} -> {ps:?}: should keep its r");
            assert!(ps.contains(expect_ipa), "{word} -> {ps:?}: expected {expect_ipa:?}");
        }
    }

    #[test]
    fn regular_er_words_still_drop_the_r() {
        // Make sure the KEEPS_R exception list doesn't overreach: ordinary
        // "-er" nouns/adjectives/infinitives must still lose the r (checked
        // as "does not end in r", since "parler"/"manger" legitimately have
        // an unrelated stem r earlier in the word).
        for word in ["danger", "boulanger", "parler", "manger"] {
            let ps = phonemize_word(word);
            assert!(!ps.ends_with('\u{281}'), "{word} -> {ps:?}: should not keep the -er's r");
        }
    }

    #[test]
    fn intervocalic_s_voices_to_z() {
        // A single "s" between two vowel sounds is /z/ - "maison",
        // "chemise" (including when the second vowel is a mute final e
        // this function truncates away before the main loop ever sees it).
        for word in ["maison", "raison", "cousin", "chemise", "franchise"] {
            let ps = phonemize_word(word);
            assert!(ps.contains('z'), "{word} -> {ps:?}: expected voiced z");
        }
    }

    #[test]
    fn doubled_s_does_not_voice() {
        for word in ["chasse", "passer", "poisson"] {
            let ps = phonemize_word(word);
            assert!(!ps.contains('z'), "{word} -> {ps:?}: doubled s should stay /s/");
        }
    }

    #[test]
    fn compound_words_keep_s_unvoiced() {
        // "parasol"/"tournesol"/"antisocial": the second element is a
        // recognizable standalone French word starting with s, so its s is
        // not treated as intervocalic the way an ordinary word's would be.
        for word in ["parasol", "tournesol"] {
            let ps = phonemize_word(word);
            assert!(!ps.contains('z'), "{word} -> {ps:?}: compound s should stay /s/");
        }
    }

    #[test]
    fn que_and_gue_endings_drop_their_mute_e() {
        // "musique"/"langue": the u after q/g is a silent hardness marker,
        // not a real vowel, so the following e is just as mute as after any
        // other consonant - these end in /ik/, /ɑ̃ɡ/, not /ikə/, /ɑ̃ɡə/.
        assert_eq!(phonemize_word("musique"), "myz\u{2c8}ik");
        assert_eq!(phonemize_word("langue"), "l\u{2c8}\u{251}\u{303}\u{261}");
    }

    #[test]
    fn word_final_e_is_mute_after_a_vowel_too() {
        // Not just after a consonant ("petite"): "vie"/"pharmacie" end in
        // the preceding vowel's own sound, not schwa.
        for word in ["vie", "amie", "pharmacie"] {
            let ps = phonemize_word(word);
            assert!(!ps.contains('\u{259}'), "{word} -> {ps:?}: final e should be fully silent");
        }
    }

    #[test]
    fn aigu_keeps_its_real_final_u() {
        // Unlike "langue"/"vague" (mute e already dropped, u silent),
        // "aigu" genuinely ends in u - it must still be pronounced.
        let ps = phonemize_word("aigu");
        assert!(ps.contains('y'), "{ps}: aigu's u is real, not a silent hardness marker");
    }

    #[test]
    fn ph_digraph_is_f() {
        for word in ["physique", "photo", "telephone", "pharmacie"] {
            let ps = phonemize_word(word);
            assert!(ps.starts_with('f') || ps.contains('f'), "{word} -> {ps:?}: ph should be /f/");
        }
    }

    #[test]
    fn doubled_r_collapses_to_one_sound() {
        // "guerre"/"terre"/"pierre": one ʁ, not two - same collapsing the
        // general consonant arm does for other doubled letters.
        for word in ["guerre", "terre", "pierre"] {
            let ps = phonemize_word(word);
            assert_eq!(
                ps.matches('\u{281}').count(),
                1,
                "{word} -> {ps:?}: expected exactly one ʁ"
            );
        }
    }

    #[test]
    fn common_subject_pronouns_are_unstressed() {
        // None of these carry a stress mark in connected speech.
        for word in ["je", "tu", "il", "on", "elle"] {
            let ps = phonemize_word(word);
            assert!(!ps.contains('\u{2c8}'), "{word} -> {ps:?}: should be unstressed");
        }
    }

    #[test]
    fn contextual_numbers_vary_by_what_follows() {
        // "six"/"dix" keep their citation /s/ standalone, drop it before a
        // consonant, and voice it to /z/ before a vowel sound - a
        // three-way alternation plain liaison-consonant-append can't
        // express.
        assert_eq!(phonemize("six").unwrap(), "s\u{2c8}is");
        assert_eq!(phonemize("six choses").unwrap(), "s\u{2c8}i \u{283}\u{2c8}os");
        assert_eq!(phonemize("six enfants").unwrap(), "s\u{2c8}iz \u{251}\u{303}f\u{2c8}\u{251}\u{303}");
    }

    #[test]
    fn huit_drops_its_t_only_before_a_consonant() {
        // The opposite shape from six/dix: present standalone AND before a
        // vowel, absent only before a consonant - never becomes a /z/.
        assert!(phonemize("huit").unwrap().ends_with('t'));
        assert!(!phonemize("huit jours").unwrap().split(' ').next().unwrap().ends_with('t'));
        assert!(phonemize("huit heures").unwrap().split(' ').next().unwrap().ends_with('t'));
    }

    #[test]
    fn vingt_only_shows_its_t_before_a_vowel() {
        // The opposite default from huit: silent both standalone and
        // before a consonant, present only before a vowel sound.
        assert!(!phonemize("vingt").unwrap().ends_with('t'));
        assert!(!phonemize("vingt choses").unwrap().split(' ').next().unwrap().ends_with('t'));
        assert!(phonemize("vingt ans").unwrap().split(' ').next().unwrap().ends_with('t'));
    }

    #[test]
    fn sept_never_varies() {
        for phrase in ["sept", "sept choses", "sept enfants"] {
            let ps = phonemize(phrase).unwrap();
            let first_word = ps.split(' ').next().unwrap();
            assert_eq!(first_word, "s\u{2c8}\u{25b}t", "{phrase} -> {ps:?}");
        }
    }

    #[test]
    fn only_emits_known_vocabulary() {
        for word in [
            "bonjour", "comment", "allez", "vous", "aujourd'hui", "château", "œuf", "est",
            "c'est", "chef", "avec", "cette", "elle", "bref", "sel", "cher", "hiver", "fer",
            "reste", "danger", "maison", "chemise", "parasol", "musique", "langue", "vie",
            "aigu", "physique", "guerre", "je", "tu", "il", "on", "six", "dix", "huit",
            "vingt", "sept",
        ] {
            let ps = phonemize_word(word);
            for c in ps.chars() {
                assert!(crate::g2p::is_known(c), "{word} -> {ps} has unknown {c:?}");
            }
        }
    }
}
