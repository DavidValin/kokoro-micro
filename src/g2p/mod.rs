//! Grapheme-to-phoneme conversion, one backend per Kokoro language.
//!
//! Kokoro-82M is not a single-alphabet model. Its nine language codes were
//! trained against three different front ends, and feeding one language's
//! phonemes to another voice is what makes a voice sound "off" rather than
//! wrong-in-an-obvious-way:
//!
//! | code  | voices        | front end                                    |
//! |-------|---------------|----------------------------------------------|
//! | a / b | `af_ am_ bf_ bm_` | eSpeak NG (`en-us` / `en-gb`) + misaki rewrites |
//! | e f h i p | `ef_ em_ ff_ hf_ hm_ if_ im_ pf_ pm_` | eSpeak NG, one voice each |
//! | z     | `zf_ zm_`     | jieba + pinyin + misaki's transcription tables |
//! | j     | `jf_ jm_`     | OpenJTalk + misaki's katakana table            |
//!
//! eSpeak has no usable Mandarin or Japanese front end for this purpose: its
//! `cmn` voice emits tone *digits*, which are not in the model's vocabulary at
//! all, and its `ja` voice cannot read kanji - it falls back to spelling them
//! out in English. Those two languages get dedicated backends.

pub(crate) mod espeak;
pub(crate) mod ja;
mod ja_data;
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
    /// Accepts the Kokoro single-letter codes, BCP-47-ish tags and the eSpeak
    /// voice names. `"en"` deliberately resolves to British English because
    /// that is what eSpeak's `en` voice is; callers who want the American
    /// accent should say `en-us`, or simply let the voice decide.
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

    /// eSpeak voice names to try for this language, best first.
    ///
    /// `espeak_SetVoiceByName` matches the voice *file* name, not the language
    /// code the `--voices` listing prints, and the two only sometimes agree:
    /// American English is the file `en-US` so `en-us` resolves, but British
    /// English is the file `en` and `en-gb` does not resolve at all. Likewise
    /// `pt-br` works while `fr-fr` does not. Rather than hard-code one spelling
    /// per language and hope, list the candidates and take the first that the
    /// installed espeak-ng actually knows.
    pub(crate) fn espeak_voices(self) -> &'static [&'static str] {
        match self {
            Lang::AmericanEnglish => &["en-us", "gmw/en-US"],
            Lang::BritishEnglish => &["en-gb", "en", "gmw/en"],
            Lang::Spanish => &["es", "roa/es"],
            Lang::French => &["fr-fr", "fr", "roa/fr"],
            Lang::Hindi => &["hi", "inc/hi"],
            Lang::Italian => &["it", "roa/it"],
            Lang::BrazilianPortuguese => &["pt-br", "roa/pt-BR"],
            Lang::Japanese | Lang::Mandarin => &[],
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
        other => espeak::phonemize(text, other.espeak_voices(), other),
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
    fn american_and_british_use_different_espeak_voices() {
        assert_eq!(Lang::AmericanEnglish.espeak_voices()[0], "en-us");
        assert_ne!(
            Lang::BritishEnglish.espeak_voices(),
            Lang::AmericanEnglish.espeak_voices()
        );
    }

    #[test]
    fn portuguese_is_brazilian() {
        assert!(
            Lang::BrazilianPortuguese
                .espeak_voices()
                .iter()
                .all(|v| v.to_ascii_lowercase().contains("pt-br")),
            "Kokoro's p voices are Brazilian; espeak's plain `pt` is European"
        );
    }

    #[test]
    fn cjk_languages_do_not_go_through_espeak() {
        assert!(Lang::Mandarin.espeak_voices().is_empty());
        assert!(Lang::Japanese.espeak_voices().is_empty());
    }

    #[test]
    fn every_espeak_language_resolves_to_an_installed_voice() {
        for lang in [
            Lang::AmericanEnglish,
            Lang::BritishEnglish,
            Lang::Spanish,
            Lang::French,
            Lang::Hindi,
            Lang::Italian,
            Lang::BrazilianPortuguese,
        ] {
            let ps = phonemize("hello", lang)
                .unwrap_or_else(|e| panic!("{:?}: {e}", lang));
            assert!(!ps.is_empty(), "{:?} produced no phonemes", lang);
        }
    }
}
