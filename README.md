# kokoro-micro

A minimal, embeddable Text-to-Speech (TTS) library for Rust using the Kokoro 82M parameter model.

> This is a reduced version of [kokoro-tiny](https://github.com/8b-is/kokoro-tiny) created by by 8b-is.

## Features

- **Minimal dependencies** - Only essential crates for TTS synthesis
- **Auto-downloading** - Model files (310MB + 27MB) download automatically to `~/.cache/k/`
- **Multiple voices** - Support for various voice styles with mixing capability
- **Speed & gain control** - Adjust speech speed and volume
- **WAV export** - Save synthesized audio to WAV files
- **Long text support** - Automatic chunking and crossfading for longer texts
- **Silent by default** - No output unless `KOKORO_DEBUG=1` is set

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
kokoro-micro = "0.2.0"
tokio = { version = "1", features = ["rt", "macros"] }
```

## Quick Start

```rust
use kokoro_micro::TtsEngine;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize TTS engine (downloads model on first run)
    let mut tts = TtsEngine::new().await?;

    // Synthesize speech
    // Parameters: text, voice (None for default), speed, gain, language
    let audio = tts.synthesize_with_options(
        "Hello world!",
        None,      // voice: None = default "af_sky"
        1.0,       // speed: 1.0 = normal
        1.0,       // gain: 1.0 = normal volume
        Some("en") // language
    )?;

    // Save to WAV file
    tts.save_wav("output.wav", &audio)?;

    Ok(())
}
```

## API Reference

### TtsEngine

Main struct for text-to-speech synthesis.

#### Methods

- **`new() -> Result<Self, String>`**  
  Create a new TTS engine. Downloads model files to `~/.cache/k/` on first run.

- **`with_paths(model_path: &str, voices_path: &str) -> Result<Self, String>`**  
  Create engine with custom model file paths.

- **`voices() -> Vec<String>`**  
  List all available voice names.

- **`synthesize_with_options(text: &str, voice: Option<&str>, speed: f32, gain: f32, lang: Option<&str>) -> Result<Vec<f32>, String>`**  
  Synthesize text to audio samples.
  - `text` - Text to synthesize
  - `voice` - Voice name (e.g., "af_sky", "af_nicole", "am_adam") or None for default
  - `speed` - Speech speed (0.5 = slower, 1.0 = normal, 2.0 = faster)
  - `gain` - Volume multiplier (0.5 = quieter, 1.0 = normal, 2.0 = louder)
  - `lang` - Language code (e.g., "en", "es", "fr") or None for default "en"

- **`save_wav(path: &str, audio: &[f32]) -> Result<(), String>`**  
  Save audio samples to a WAV file.

### Voice Mixing

You can mix multiple voices by using weighted combinations:

```rust
// Mix 40% af_sky + 50% af_nicole
let audio = tts.synthesize_with_options(
    "Hello!",
    Some("af_sky.4+af_nicole.5"),
    1.0,
    1.0,
    Some("en")
)?;
```

### Available Voices

Common voices include:
- `af_sky` (default) - Female, gentle
- `af_nicole` - Female
- `af_bella` - Female
- `am_adam` - Male
- `am_michael` - Male

Use `tts.voices()` to list all available voices.

## Debug Logging

By default, kokoro-micro runs silently with no console output. To enable debug logging (model download progress, synthesis details, etc.), set the `KOKORO_DEBUG` environment variable:

```bash
# Enable debug logging
KOKORO_DEBUG=1 cargo run --example simple

# Or in your code
std::env::set_var("KOKORO_DEBUG", "1");
```

Debug logging shows:
- Model download progress
- Long-form synthesis chunking information
- Phoneme conversion details
- Audio generation statistics

## Example

See `examples/simple.rs`:

```bash
# Run without debug output
cargo run --example simple

# Run with debug output
KOKORO_DEBUG=1 cargo run --example simple
```

## Features

### Optional Features

- **`cuda`** - Enable CUDA acceleration for ONNX Runtime

```toml
[dependencies]
kokoro-micro = { version = "0.2.0", features = ["cuda"] }
```

## Model Files

Model files are automatically downloaded on first use to `$HOME/.cache/k/`:
- `$HOME/.cache/k/0.onnx` (310MB) - Kokoro ONNX model
- `$HOME/.cache/k/0.bin` (27MB) - Voice embeddings

The same cache directory is used on **all platforms** (Linux, macOS, Windows):
- **Linux/macOS**: `$HOME/.cache/k/` (e.g., `/home/user/.cache/k/`)
- **Windows**: `%USERPROFILE%/.cache/k/` (e.g., `C:\Users\Username\.cache\k\`)

Files are cached and shared across all applications using kokoro-micro.

## License

Apache-2.0

## Credits

Built with the Kokoro 82M parameter TTS model.
Reduced version from [kokoro-tiny](https://github.com/8b-is/kokoro-tiny) by 8b-is.
