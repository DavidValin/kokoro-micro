//! Japanese front end.
//!
//! A port of misaki's `cutlet` path - the one `JAG2P` uses by default, and the
//! one whose output alphabet matches Kokoro's vocabulary. Readings come from
//! `jpreprocess`, a Rust rewrite of OpenJTalk's text processor, which is the
//! same engine misaki reaches through `pyopenjtalk`.
//!
//! 1. Normalize width and script variants.
//! 2. Tokenize; each token carries a katakana pronunciation, with OpenJTalk's
//!    rendaku and compound rules already applied. Numbers are read here too.
//! 3. Map kana to phonemes one mora at a time, with the neighbours in hand:
//!    digraphs (きゃ), the sokuon っ, the moraic nasal ん - whose place of
//!    articulation follows the next consonant - and the long vowel ー all
//!    depend on context.
//!
//! Two deliberate departures from misaki, both because the model is the final
//! authority on what a phoneme is:
//!
//! * Parentheses stay as `(` and `)`. misaki rewrites them to `«` and `»`,
//!   which Kokoro has no token for, so they would simply be dropped.
//! * Latin runs are phonemized as English rather than passed through as bare
//!   letters, which the model would otherwise spell out.

use std::sync::OnceLock;

use jpreprocess::{kind::JPreprocessDictionaryKind, JPreprocess, SystemDictionaryConfig};

use super::ja_data::KANA;

type Engine = JPreprocess<jpreprocess::DefaultTokenizer>;

fn engine() -> Option<&'static Engine> {
    static ENGINE: OnceLock<Option<Engine>> = OnceLock::new();
    ENGINE
        .get_or_init(|| {
            let dict = SystemDictionaryConfig::Bundled(JPreprocessDictionaryKind::NaistJdic)
                .load()
                .ok()?;
            Some(JPreprocess::with_dictionaries(dict, None))
        })
        .as_ref()
}

fn lookup(kana: &str) -> Option<&'static str> {
    KANA.binary_search_by(|(k, _)| (*k).cmp(kana))
        .ok()
        .map(|i| KANA[i].1)
}

/// Katakana -> hiragana, leaving everything else (ー included) alone.
fn kata_to_hira(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '\u{30a1}'..='\u{30f6}' => char::from_u32(c as u32 - 0x60).unwrap_or(c),
            _ => c,
        })
        .collect()
}

/// Small kana that combine with the preceding mora rather than standing alone.
fn is_sutegana(c: char) -> bool {
    matches!(c, 'ゃ' | 'ゅ' | 'ょ' | 'ぁ' | 'ぃ' | 'ぅ' | 'ぇ' | 'ぉ')
}

/// Fold the Katakana Phonetic Extensions and fullwidth ASCII onto the ordinary
/// forms, so the table only has to carry one spelling of each mora.
fn normalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        let replacement = match c {
            '\u{31f0}' => "ク",
            '\u{31f1}' => "シ",
            '\u{31f2}' => "ス",
            '\u{31f3}' => "ト",
            '\u{31f4}' => "ヌ",
            '\u{31f5}' => "ハ",
            '\u{31f6}' => "ヒ",
            '\u{31f7}' => "フ",
            '\u{31f8}' => "ヘ",
            '\u{31f9}' => "ホ",
            '\u{31fa}' => "ム",
            '\u{31fb}' => "ラ",
            '\u{31fc}' => "リ",
            '\u{31fd}' => "ル",
            '\u{31fe}' => "レ",
            '\u{31ff}' => "ロ",
            // Fullwidth ASCII -> halfwidth, so digits reach the number reader
            // and Latin reaches the English front end.
            '\u{ff01}'..='\u{ff5e}' => {
                out.push(char::from_u32(c as u32 - 0xfee0).unwrap_or(c));
                continue;
            }
            _ => {
                out.push(c);
                continue;
            }
        };
        out.push_str(replacement);
    }
    out
}

/// One mora of hiragana in context -> phonemes. Port of cutlet's
/// `_get_single_mapping`.
fn mora_to_phonemes(prev: Option<char>, cur: char, next: Option<char>) -> String {
    // A digraph is emitted when its second half is reached, so the first half
    // produces nothing on its own.
    if let Some(p) = prev {
        let pair: String = [p, cur].iter().collect();
        if let Some(v) = lookup(&pair) {
            return v.to_string();
        }
    }
    if let Some(n) = next {
        let pair: String = [cur, n].iter().collect();
        if lookup(&pair).is_some() {
            return String::new();
        }
    }
    // Palatalized moras with no entry of their own: keep the consonant and
    // take the small kana's vowel, so きゃ is k + a.
    if let Some(n) = next.filter(|c| is_sutegana(*c)) {
        if cur == 'っ' {
            return String::new();
        }
        let base = lookup(&cur.to_string()).unwrap_or("");
        let small = lookup(&n.to_string()).unwrap_or("");
        let keep = base.chars().count().saturating_sub(1);
        let mut merged: String = base.chars().take(keep).collect();
        merged.push_str(small);
        return merged;
    }
    if is_sutegana(cur) {
        return String::new();
    }
    match cur {
        '\u{30fc}' => "ː".to_string(), // 長音符
        'っ' => "ʔ".to_string(),       // 促音
        'ん' => {
            // The moraic nasal takes the place of articulation of whatever
            // follows: m before m/p/b, ŋ before k/g, ɲ before palatals,
            // n before coronals, uvular ɴ elsewhere.
            let following = next.and_then(|n| lookup(&n.to_string())).unwrap_or("");
            if following.starts_with('ɲ')
                || following.starts_with('ʨ')
                || following.starts_with('ʥ')
            {
                return "ɲ".to_string();
            }
            match following.chars().next() {
                Some('m') | Some('p') | Some('b') => "m",
                Some('k') | Some('ɡ') => "ŋ",
                Some('n') | Some('t') | Some('d') | Some('ɾ') | Some('z') => "n",
                _ => "ɴ",
            }
            .to_string()
        }
        _ => lookup(&cur.to_string()).unwrap_or("").to_string(),
    }
}

fn word_to_phonemes(hira: &str) -> String {
    let chars: Vec<char> = hira.chars().collect();
    let mut out = String::new();
    for (i, &c) in chars.iter().enumerate() {
        let prev = if i > 0 { Some(chars[i - 1]) } else { None };
        let next = chars.get(i + 1).copied();
        out.push_str(&mora_to_phonemes(prev, c, next));
    }
    out
}

/// Whether a token is punctuation or a symbol rather than something spoken.
fn is_symbolic(surface: &str) -> bool {
    !surface.is_empty()
        && surface.chars().all(|c| {
            !c.is_alphanumeric()
                && !matches!(c,
                    '\u{3040}'..='\u{30ff}'      // kana
                    | '\u{3400}'..='\u{4dbf}'    // CJK ext A
                    | '\u{4e00}'..='\u{9fff}'    // CJK
                )
        })
}

/// Punctuation that closes a phrase: no space before it, one space after.
fn is_closing(ps: &str) -> bool {
    matches!(ps, ")" | "." | "," | "?" | "!" | ":" | ";" | "\u{201d}")
}

/// Punctuation that opens one: a space before it, none after.
fn is_opening(ps: &str) -> bool {
    matches!(ps, "(" | "\u{201c}")
}

/// Japanese text -> Kokoro phonemes.
pub(crate) fn phonemize(text: &str) -> String {
    let Some(engine) = engine() else {
        return String::new();
    };
    let text = normalize(text);
    let Ok(njd) = engine.text_to_njd(&text) else {
        return String::new();
    };

    // (phonemes, whether a space follows, part of speech)
    let mut tokens: Vec<(String, bool, String)> = Vec::new();
    let mut latin = String::new();

    fn flush_latin(latin: &mut String, tokens: &mut Vec<(String, bool, String)>) {
        if !latin.trim().is_empty() {
            let lang = super::Lang::AmericanEnglish;
            if let Ok(ps) = super::en::phonemize(latin.trim(), lang) {
                if !ps.is_empty() {
                    tokens.push((ps, true, String::new()));
                }
            }
        }
        latin.clear();
    }

    for node in njd.nodes.iter() {
        let surface = node.get_string();
        if surface.trim().is_empty() {
            continue;
        }
        // Latin that survived the reader is read as English.
        if surface.chars().all(|c| c.is_ascii_alphabetic() || c == '\'') {
            latin.push_str(surface);
            latin.push(' ');
            continue;
        }
        flush_latin(&mut latin, &mut tokens);

        // OpenJTalk marks the accent nucleus inside the reading; that is
        // prosody bookkeeping, not a phoneme.
        // The katakana reading. The accent mark OpenJTalk leaves in it is
        // prosody bookkeeping rather than a phoneme.
        let pron: String = node
            .get_pron()
            .to_pure_string()
            .chars()
            .filter(|c| !matches!(c, '\u{2019}' | '\''))
            .collect();
        let pos = node.get_pos().to_string();

        // Punctuation is taken from the surface: the reader hands back a
        // generic pause marker for it, so a full stop would come out as a
        // comma. For anything spoken the reading wins instead - a kana surface
        // is not self-explanatory, since the topic particle は is read わ and
        // looking the character up would say "ha".
        let ps = if is_symbolic(surface) {
            lookup(surface)
                .map(str::to_string)
                .unwrap_or_else(|| surface.chars().filter(|c| super::is_known(*c)).collect())
        } else if pron.trim().is_empty() {
            lookup(surface).unwrap_or("").to_string()
        } else {
            word_to_phonemes(&kata_to_hira(&pron))
        };
        if ps.is_empty() {
            continue;
        }
        // An auxiliary verb is part of the word it inflects, not a word of
        // its own: 行き+まし+た is one token to a listener, and jpreprocess
        // hands ましょう back as ましょ + う. Leaving the default space between
        // them makes the model pause in the middle of a verb.
        if pos.starts_with("助動詞") {
            if let Some((last_ps, space, last_pos)) = tokens.last_mut() {
                if last_pos.starts_with("動詞")
                    || last_pos.starts_with("助動詞")
                    || last_pos.starts_with("形容詞")
                {
                    last_ps.push_str(&ps);
                    *space = true;
                    *last_pos = pos;
                    continue;
                }
            }
        }
        if is_opening(&ps) {
            if let Some(last) = tokens.last_mut() {
                last.1 = true;
            }
            tokens.push((ps, false, pos));
        } else if is_closing(&ps) {
            if let Some(last) = tokens.last_mut() {
                last.1 = false;
            }
            tokens.push((ps, true, pos));
        } else {
            tokens.push((ps, true, pos));
        }
    }
    flush_latin(&mut latin, &mut tokens);

    let mut out = String::new();
    for (ps, space, _) in &tokens {
        out.push_str(ps);
        if *space {
            out.push(' ');
        }
    }

    // The sokuon lengthens the consonant that follows it, so it never has a
    // word break on either side.
    let out = out.replace(" ʔ", "ʔ").replace("ʔ ", "ʔ");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kana_table_is_sorted_for_binary_search() {
        assert!(KANA.windows(2).all(|w| w[0].0 < w[1].0));
    }

    #[test]
    fn maps_basic_moras() {
        assert_eq!(word_to_phonemes("あい"), "ai");
        assert_eq!(word_to_phonemes("し"), "ɕi");
        assert_eq!(word_to_phonemes("つ"), "ʦɨ");
        assert_eq!(word_to_phonemes("きゃ"), "kʲa");
    }

    #[test]
    fn moraic_nasal_assimilates_to_what_follows() {
        assert!(word_to_phonemes("んま").starts_with('m'));
        assert!(word_to_phonemes("んか").starts_with('ŋ'));
        assert!(word_to_phonemes("んた").starts_with('n'));
        assert!(word_to_phonemes("ん").starts_with('ɴ'));
    }

    #[test]
    fn sokuon_and_long_vowel_have_their_own_symbols() {
        assert!(word_to_phonemes("かった").contains('ʔ'));
        assert!(word_to_phonemes(&kata_to_hira("ラーメン")).contains('ː'));
    }

    #[test]
    fn a_verb_and_its_auxiliaries_are_one_word() {
        // jpreprocess splits ましょう into ましょ + う, and 行きました into
        // 行き + まし + た. A space between them is a pause mid-verb, so the
        // whole inflected form has to come back as a single space-free run.
        for text in ["会いましょう。", "行きました。", "食べられなかった。"] {
            let ps = phonemize(text);
            let longest = ps
                .split(|c: char| c.is_whitespace() || c == '.')
                .map(|w| w.chars().count())
                .max()
                .unwrap_or(0);
            assert!(
                longest >= 6,
                "{text} -> {ps}: the verb looks broken into separate words"
            );
        }
    }

    #[test]
    fn every_phoneme_is_in_the_model_vocabulary() {
        for (kana, phonemes) in KANA {
            for c in phonemes.chars() {
                assert!(
                    super::super::is_known(c),
                    "{kana} -> {phonemes} contains {c:?}, which the model has no token for"
                );
            }
        }
    }

    #[test]
    fn phonemizes_a_sentence() {
        let ps = phonemize("こんにちは。今日はいい天気ですね。");
        assert!(!ps.is_empty(), "no output - is the bundled dictionary available?");
        assert!(!ps.contains('A'), "English diphthong token in {ps}");
        let unknown: Vec<char> = ps.chars().filter(|c| !super::super::is_known(*c)).collect();
        assert!(unknown.is_empty(), "not in the vocabulary: {unknown:?} in {ps}");
    }
}
