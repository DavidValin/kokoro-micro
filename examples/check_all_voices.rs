//! Synthesize with every voice the model ships and report what happened.
//!
//!     cargo run --release --example check_all_voices
//!
//! Checks, for each of the 54 voices:
//!   * the voice name routes to a language
//!   * a sample in that language phonemizes into vocabulary characters only
//!   * synthesis returns plausible audio (non-empty, finite, not silence)

use kokoro_micro::{g2p, Lang, TtsEngine};

fn sample(lang: Lang) -> &'static str {
    match lang {
        Lang::AmericanEnglish | Lang::BritishEnglish => {
            "Hello, I made a mistake today. Go outside now, and take the boat home."
        }
        Lang::Spanish => "Hola, ¿cómo estás hoy? Vamos al parque esta tarde.",
        Lang::French => "Bonjour, comment allez-vous aujourd'hui ? Allons au parc.",
        Lang::Italian => "Ciao, come stai oggi? Andiamo al parco questo pomeriggio.",
        Lang::BrazilianPortuguese => "Olá, como você está hoje? Vamos ao parque esta tarde.",
        Lang::Hindi => "नमस्ते, आप आज कैसे हैं? आज मौसम बहुत अच्छा है।",
        Lang::Mandarin => "你好，世界。我们今天去公园散步，好吗？",
        Lang::Japanese => "こんにちは。今日はいい天気ですね。",
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let tts = TtsEngine::new().await.expect("engine");
    let mut voices = tts.voices();
    voices.sort();

    let mut failures = Vec::new();
    let mut by_lang: std::collections::BTreeMap<char, usize> = Default::default();

    for voice in &voices {
        let Some(lang) = Lang::from_voice(voice) else {
            failures.push(format!("{voice}: name does not route to a language"));
            println!("FAIL {voice:16} unroutable");
            continue;
        };
        *by_lang.entry(lang.code()).or_default() += 1;
        let text = sample(lang);

        let phonemes = match g2p::phonemize(text, lang) {
            Ok(p) => p,
            Err(e) => {
                failures.push(format!("{voice}: g2p failed: {e}"));
                println!("FAIL {voice:16} [{}] g2p: {e}", lang.code());
                continue;
            }
        };
        let unknown: Vec<char> = phonemes.chars().filter(|c| !g2p::is_known(*c)).collect();
        if !unknown.is_empty() {
            failures.push(format!("{voice}: phonemes outside vocabulary: {unknown:?}"));
        }

        match tts.synthesize_with_options(text, Some(voice), 1.0, 1.0, None) {
            Ok(audio) => {
                let seconds = audio.len() as f32 / 24000.0;
                let peak = audio.iter().fold(0.0f32, |m, s| m.max(s.abs()));
                let finite = audio.iter().all(|s| s.is_finite());
                let ok = seconds > 1.0 && peak > 0.01 && finite;
                if !ok {
                    failures.push(format!(
                        "{voice}: {seconds:.2}s peak {peak:.3} finite {finite}"
                    ));
                }
                println!(
                    "{}  {voice:16} [{}] {seconds:>5.2}s peak {peak:.2}",
                    if ok { "ok  " } else { "FAIL" },
                    lang.code()
                );
            }
            Err(e) => {
                failures.push(format!("{voice}: synthesis failed: {e}"));
                println!("FAIL {voice:16} [{}] {e}", lang.code());
            }
        }
    }

    println!("\n{} voices, by language: {by_lang:?}", voices.len());
    if failures.is_empty() {
        println!("all voices synthesized");
    } else {
        println!("\n{} FAILURES:", failures.len());
        for f in &failures {
            println!("  {f}");
        }
        std::process::exit(1);
    }
}
