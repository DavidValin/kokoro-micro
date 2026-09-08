//! Speak one line of text and write it to a WAV.
//!
//!     cargo run --release --example say -- <voice> <text> [out.wav]
//!
//! The language comes from the voice name, so there is nothing else to pass:
//!
//!     cargo run --release --example say -- zf_xiaoni "你好，世界。"
//!     cargo run --release --example say -- jm_kumo  "こんにちは。"
//!
//! Defaults to /tmp/say.wav. Prints the phonemes it used, which is usually
//! what you want to look at when something sounds wrong.

use kokoro_micro::{Lang, TtsEngine};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(voice), Some(text)) = (args.next(), args.next()) else {
        eprintln!("usage: say <voice> <text> [out.wav]");
        eprintln!("       say zf_xiaoni \"你好，世界。\"");
        std::process::exit(2);
    };
    let out = args.next().unwrap_or_else(|| "/tmp/say.wav".to_string());

    let mut tts = TtsEngine::new().await.expect("engine");

    match Lang::from_voice(&voice) {
        Some(lang) => println!("voice {voice}, language {}", lang.code()),
        None => println!("voice {voice} is not a known Kokoro voice name"),
    }
    match tts.phonemize(&text, Some(&voice)) {
        Ok(ps) => println!("phonemes: {ps}"),
        Err(e) => eprintln!("phonemes: {e}"),
    }

    let audio = match tts.synthesize_with_options(&text, Some(&voice), 1.0, 1.0, None) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("synthesis failed: {e}");
            std::process::exit(1);
        }
    };
    tts.save_wav(&out, &audio).expect("write wav");
    println!(
        "wrote {out} ({:.2}s, 24 kHz mono)",
        audio.len() as f32 / 24000.0
    );
}
