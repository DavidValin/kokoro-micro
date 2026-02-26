//! Simple example showing basic TTS usage
//!
//! Run with debug logging:
//! KOKORO_DEBUG=1 cargo run --example simple

use kokoro_micro::TtsEngine;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("🎤 kokoro-micro simple example");
    println!("==============================\n");
    
    if std::env::var("KOKORO_DEBUG").is_ok() {
        println!("Debug logging enabled\n");
    }

    // Initialize TTS engine
    let mut tts = TtsEngine::new().await?;

    // Simple text synthesis
    let text = "Hello from kokoro-micro! This is a minimal text to speech engine.";
    println!("Synthesizing: \"{}\"\n", text);

    // Use synthesize_with_options: text, voice, speed, gain, lang
    let audio = tts.synthesize_with_options(text, None, 1.0, 1.0, Some("en"))?;
    println!("✅ Generated {} audio samples", audio.len());

    // Save to file
    tts.save_wav("simple_output.wav", &audio)?;
    println!("💾 Saved to: simple_output.wav");

    Ok(())
}
