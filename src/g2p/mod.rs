//! Grapheme-to-phoneme conversion, one backend per Kokoro language.
//!
//! Kokoro-82M is not a single-alphabet model. Its nine language codes were
//! trained against different front ends, and feeding one language's
//! phonemes to another voice is what makes a voice sound "off" rather than
//! wrong-in-an-obvious-way:
//!
//! | code      | voices                                | front end                          |
//! |-----------|---------------------------------------|-------------------------------------|
//! | a / b     | `af_ am_ bf_ bm_`                     | [`en`]: misaki's dictionary + fallback rules |
//! | e / i / f / p | `ef_ em_ if_ im_ ff_ ff_ pf_ pm_`  | [`romance`]: hand-written rules per language |
//! | h         | `hf_ hm_`                             | [`hi`]: Devanagari letter-to-sound rules |
//! | z         | `zf_ zm_`                             | jieba + pinyin + misaki's transcription tables |
//! | j         | `jf_ jm_`                             | OpenJTalk + misaki's katakana table |
//!
//! English is dictionary-first using misaki's own (Apache-2.0) lexicons, and
//! the rest are small from-scratch rule sets, since
//! Spanish/Italian/French/Portuguese/Hindi orthography is regular enough
//! that spelling determines pronunciation almost completely. See each
//! submodule's docs for what is and is not covered.

pub(crate) mod en;
pub(crate) mod hi;
pub(crate) mod ja;
mod ja_data;
pub(crate) mod romance;
mod text;
pub(crate) mod zh;
mod zh_syllables;

/// A Kokoro language, as selected by the voice being spoken.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    AmericanEnglish,
    BritishEnglish,
    Spanish,
    French,
    Hindi,
    Italian,
    BrazilianPortuguese,
    Japanese,
    Mandarin,
}

impl Lang {
    /// The language a Kokoro voice was trained for, from its name prefix.
    ///
    /// Voice names encode this directly - `af_heart` is American English,
    /// `zf_xiaoni` is Mandarin - so the caller never has to say. Mixed voices
    /// (`af_bella.5+af_sky.5`) are resolved from the first component.
    pub fn from_voice(voice: &str) -> Option<Self> {
        let first = voice.split('+').next().unwrap_or(voice);
        let name = first.split('.').next().unwrap_or(first);
        let bytes = name.as_bytes();
        if bytes.len() < 3 || bytes[1] != b'f' && bytes[1] != b'm' || bytes[2] != b'_' {
            return None;
        }
        Some(match bytes[0] {
            b'a' => Lang::AmericanEnglish,
            b'b' => Lang::BritishEnglish,
            b'e' => Lang::Spanish,
            b'f' => Lang::French,
            b'h' => Lang::Hindi,
            b'i' => Lang::Italian,
            b'p' => Lang::BrazilianPortuguese,
            b'j' => Lang::Japanese,
            b'z' => Lang::Mandarin,
            _ => return None,
        })
    }

    /// Best-effort match for a language name a caller passed explicitly.
    ///
    /// Accepts the Kokoro single-letter codes and BCP-47-ish tags. `"en"`
    /// resolves to British English; callers who want the American accent
    /// should say `en-us`, or simply let the voice decide via
    /// [`Lang::from_voice`].
    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.trim().to_ascii_lowercase();
        Some(match name.as_str() {
            "a" | "en-us" | "en_us" | "american english" => Lang::AmericanEnglish,
            "b" | "en" | "en-gb" | "en_gb" | "british english" => Lang::BritishEnglish,
            "e" | "es" | "spanish" => Lang::Spanish,
            "f" | "fr" | "fr-fr" | "french" => Lang::French,
            "h" | "hi" | "hindi" => Lang::Hindi,
            "i" | "it" | "italian" => Lang::Italian,
            "p" | "pt" | "pt-br" | "portuguese" => Lang::BrazilianPortuguese,
            "j" | "ja" | "jp" | "japanese" => Lang::Japanese,
            "z" | "zh" | "cmn" | "zh-cn" | "mandarin" => Lang::Mandarin,
            _ => return None,
        })
    }

    /// The Kokoro language code (`a`, `b`, `e`, `f`, `h`, `i`, `j`, `p`, `z`).
    pub fn code(self) -> char {
        match self {
            Lang::AmericanEnglish => 'a',
            Lang::BritishEnglish => 'b',
            Lang::Spanish => 'e',
            Lang::French => 'f',
            Lang::Hindi => 'h',
            Lang::Italian => 'i',
            Lang::Japanese => 'j',
            Lang::BrazilianPortuguese => 'p',
            Lang::Mandarin => 'z',
        }
    }
}

/// Whether the model has a token for this phoneme.
///
/// Anything else is dropped before inference, so this is the check to make
/// when validating phonemes you produced yourself.
pub fn is_known(phoneme: char) -> bool {
    crate::vocab::vocab().contains_key(&phoneme)
}

/// Convert text to Kokoro phonemes for `lang`.
///
/// The output is Kokoro's own alphabet - misaki's symbol set rather than plain
/// IPA - so `A` is /eɪ/, `ʧ` is /tʃ/ and Mandarin tones are `→ ↗ ↓ ↘`.
pub fn phonemize(text: &str, lang: Lang) -> Result<String, String> {
    match lang {
        Lang::Mandarin => Ok(zh::phonemize(text)),
        Lang::Japanese => Ok(ja::phonemize(text)),
        Lang::AmericanEnglish | Lang::BritishEnglish => en::phonemize(text, lang),
        Lang::Spanish => romance::es::phonemize(text),
        Lang::Italian => romance::it::phonemize(text),
        Lang::French => romance::fr::phonemize(text),
        Lang::BrazilianPortuguese => romance::pt::phonemize(text),
        Lang::Hindi => hi::phonemize(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routes_voices_to_their_training_language() {
        assert_eq!(Lang::from_voice("af_heart"), Some(Lang::AmericanEnglish));
        assert_eq!(Lang::from_voice("am_adam"), Some(Lang::AmericanEnglish));
        assert_eq!(Lang::from_voice("bf_emma"), Some(Lang::BritishEnglish));
        assert_eq!(Lang::from_voice("ef_dora"), Some(Lang::Spanish));
        assert_eq!(Lang::from_voice("ff_siwis"), Some(Lang::French));
        assert_eq!(Lang::from_voice("hm_omega"), Some(Lang::Hindi));
        assert_eq!(Lang::from_voice("im_nicola"), Some(Lang::Italian));
        assert_eq!(Lang::from_voice("pf_dora"), Some(Lang::BrazilianPortuguese));
        assert_eq!(Lang::from_voice("jm_kumo"), Some(Lang::Japanese));
        assert_eq!(Lang::from_voice("zf_xiaoni"), Some(Lang::Mandarin));
    }

    #[test]
    fn resolves_mixed_and_weighted_voices_from_the_first_component() {
        assert_eq!(
            Lang::from_voice("bm_daniel.5+am_puck.5"),
            Some(Lang::BritishEnglish)
        );
        assert_eq!(Lang::from_voice("af_sky.8"), Some(Lang::AmericanEnglish));
    }

    #[test]
    fn rejects_names_that_are_not_kokoro_voices() {
        assert_eq!(Lang::from_voice("fallback"), None);
        assert_eq!(Lang::from_voice("qq_nobody"), None);
        assert_eq!(Lang::from_voice("a"), None);
    }

    #[test]
    fn every_language_produces_phonemes_known_to_the_model() {
        for (lang, sample) in [
            (Lang::AmericanEnglish, "hello"),
            (Lang::BritishEnglish, "hello"),
            (Lang::Spanish, "hola"),
            (Lang::French, "bonjour"),
            (Lang::Hindi, "नमस्ते"),
            (Lang::Italian, "ciao"),
            (Lang::BrazilianPortuguese, "olá"),
        ] {
            let ps = phonemize(sample, lang).unwrap_or_else(|e| panic!("{:?}: {e}", lang));
            assert!(!ps.is_empty(), "{:?} produced no phonemes", lang);
            for c in ps.chars() {
                assert!(is_known(c), "{lang:?}: {c:?} in {ps:?} is not in the model vocabulary");
            }
        }
    }
}

#[cfg(test)]
mod cross_language_regression {
    use super::*;

    /// A realistic sentence per language plus a battery of edge cases
    /// (empty, whitespace-only, bare punctuation, numbers), checked against
    /// every backend at once - the individual per-module tests cover a
    /// backend's own rules in detail, this one is the "did wiring a language
    /// through `phonemize` actually work" smoke test.
    #[test]
    fn every_language_handles_its_sample_and_edge_cases() {
        let samples: &[(Lang, &str)] = &[
            (Lang::AmericanEnglish, "Hello, I made a mistake today. Go outside now! It costs $5.50, that's 24 apples."),
            (Lang::BritishEnglish, "Hello, I made a mistake today. Go outside now!"),
            (Lang::Spanish, "Hola, ¿cómo estás hoy? Vamos al parque esta tarde."),
            (Lang::French, "Bonjour, comment allez-vous aujourd'hui ? Allons au parc."),
            (Lang::Italian, "Ciao, come stai oggi? Andiamo al parco questo pomeriggio."),
            (Lang::BrazilianPortuguese, "Olá, como você está hoje? Vamos ao parque esta tarde."),
            (Lang::Hindi, "नमस्ते, आप आज कैसे हैं? आज मौसम बहुत अच्छा है।"),
            (Lang::Mandarin, "你好，世界。我们今天去公园散步，好吗？"),
            (Lang::Japanese, "こんにちは。今日はいい天気ですね。"),
        ];
        for (lang, text) in samples {
            let ps = phonemize(text, *lang).unwrap_or_else(|e| panic!("{lang:?} sample failed: {e}"));
            assert!(!ps.is_empty(), "{lang:?} produced empty output for sample");
            let unknown: Vec<char> = ps.chars().filter(|c| !is_known(*c)).collect();
            assert!(unknown.is_empty(), "{lang:?}: unknown phonemes {unknown:?} in {ps:?}");

            // Edge cases: must not panic, on any language.
            for edge in ["", "   ", "!!!???", "123", "42.5", "-7"] {
                let _ = phonemize(edge, *lang);
            }
        }
    }
}
