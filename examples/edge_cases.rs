//! Awkward inputs, to see that nothing panics or comes back empty.
//!
//!     cargo run --release --example edge_cases

use kokoro_micro::TtsEngine;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let tts = TtsEngine::new().await.expect("engine");

    let cases: &[(&str, &str, &str)] = &[
        ("digits, en", "af_heart", "I paid 1,234 dollars on 3 May 2025."),
        ("digits, zh", "zf_xiaoni", "我有3个苹果，一共25元。"),
        ("digits, ja", "jm_kumo", "3時に会いましょう。"),
        ("latin in zh", "zf_xiaoni", "我的WiFi密码是什么？"),
        ("latin in ja", "jm_kumo", "WiFiのパスワードは何ですか。"),
        ("voice mixing", "af_bella.5+af_sky.5", "Mixing two voices together."),
        ("no terminator", "af_heart", &"one thing, and another thing, ".repeat(60)),
        ("no punctuation at all, zh", "zf_xiaoni", &"今天天气很好我们去公园散步吧".repeat(12)),
        ("newlines", "af_heart", "First line.\nSecond line.\n\nFourth line."),
        ("quotes and parens", "af_heart", "She said (quietly) \"go now\"."),
        ("emoji and symbols", "af_heart", "Hello 👋 world — 100% done!"),
        ("only punctuation", "af_heart", "..."),
        ("single word", "af_heart", "Hello"),
        ("empty", "af_heart", ""),
        ("whitespace only", "af_heart", "   \n  "),
    ];

    let mut bad = 0;
    for (name, voice, text) in cases {
        match tts.synthesize_with_options(text, Some(voice), 1.0, 1.0, None) {
            Ok(audio) => {
                let seconds = audio.len() as f32 / 24000.0;
                let finite = audio.iter().all(|s| s.is_finite());
                if !finite {
                    bad += 1;
                }
                println!(
                    "ok    {name:26} {seconds:>6.2}s{}",
                    if finite { "" } else { "  NON-FINITE SAMPLES" }
                );
            }
            // Empty input has nothing to say; that is a legitimate error, not
            // a crash. Anything else is not.
            Err(e) => {
                let expected = text.trim().is_empty();
                if !expected {
                    bad += 1;
                }
                println!(
                    "{}  {name:26} {e}",
                    if expected { "ok  " } else { "FAIL" }
                );
            }
        }
    }
    println!("\n{}", if bad == 0 { "no failures" } else { "FAILURES PRESENT" });
    if bad > 0 {
        std::process::exit(1);
    }
}
