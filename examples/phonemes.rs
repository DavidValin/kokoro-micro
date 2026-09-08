//! Print the phonemes this crate produces, without touching the model.
//!
//!     cargo run --example phonemes
//!
//! Useful for eyeballing G2P changes: every character printed should be one
//! the Kokoro vocabulary knows, and anything that is not is flagged.

use kokoro_micro::{g2p, Lang};

fn main() {
    let samples: &[(&str, &str)] = &[
        ("af_heart", "Hello, I made a mistake today. Go outside now!"),
        ("bf_emma", "Hello, I made a mistake today. Go outside now!"),
        ("ef_dora", "Hola, ¿cómo estás hoy? Vamos al parque."),
        ("ff_siwis", "Bonjour, comment allez-vous aujourd'hui ?"),
        ("if_sara", "Ciao, come stai oggi? Andiamo al parco."),
        ("pf_dora", "Olá, como você está hoje? Vamos ao parque."),
        ("hf_alpha", "नमस्ते, आप आज कैसे हैं?"),
        ("zf_xiaoni", "你好，世界。我们今天去公园散步，好吗？"),
        ("zm_yunxi", "他在银行工作，长江大桥有3公里。"),
        ("jm_kumo", "こんにちは。今日はいい天気ですね。"),
    ];

    for (voice, text) in samples {
        let lang = Lang::from_voice(voice).expect("known voice");
        print!("{voice:11} [{}] ", lang.code());
        match g2p::phonemize(text, lang) {
            Ok(ps) => {
                let unknown: Vec<char> = ps.chars().filter(|c| !g2p::is_known(*c)).collect();
                println!("{ps}");
                if !unknown.is_empty() {
                    println!("{:>16}not in vocabulary: {unknown:?}", "");
                }
            }
            Err(e) => println!("unavailable: {e}"),
        }
    }
}
