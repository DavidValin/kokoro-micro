# kokoro-micro

A minimal, embeddable Text-to-Speech (TTS) library for Rust using the Kokoro 82M parameter model.

> This is a reduced version of [kokoro-tiny](https://github.com/8b-is/kokoro-tiny) created by by 8b-is.

## Features

- **All nine Kokoro languages** - the voice picks the language and the front end
- **Minimal dependencies** - Only essential crates for TTS synthesis
- **Auto-downloading** - Model files (310MB + 27MB) download automatically to `~/.cache/k/`
- **Multiple voices** - Support for various voice styles with mixing capability
- **Speed & gain control** - Adjust speech speed and volume
- **WAV export** - Save synthesized audio to WAV files
- **Long text support** - Automatic chunking and crossfading for longer texts
- **Silent by default** - No output unless `KOKORO_DEBUG=1` is set

## Languages

The voice decides the language, so you rarely have to say: `af_heart` is
American English, `bf_emma` British, `zf_xiaoni` Mandarin. Pass `None` for
`lang` and the right front end is selected from the voice name.

All nine are built in; there is nothing to enable.

| Kokoro code | Voices | Front end |
|---|---|---|
| `a` | `af_* am_*` (20) | dictionary-first English (misaki's lexicons) + fallback rules |
| `b` | `bf_* bm_*` (8) | dictionary-first English (misaki's lexicons) + fallback rules |
| `e` `i` `f` `p` | `ef_ em_ if_ im_ ff_ fm_ pf_ pm_` (13) | hand-written per-language letter-to-sound rules |
| `h` | `hf_ hm_` | Devanagari letter-to-sound rules |
| `z` | `zf_* zm_*` (8) | jieba + pinyin + misaki's transcription tables |
| `j` | `jf_* jm_*` (5) | OpenJTalk (`jpreprocess`) + misaki's kana table |

This is not incidental detail. Kokoro-82M was trained against different
grapheme-to-phoneme front ends, each speaking a different phoneme alphabet, and
the model's vocabulary is only 114 tokens wide. Anything outside it is dropped
before inference, so a language handled by the wrong front end does not fail
loudly - it comes back mumbling. Mandarin in particular needs tone contours
written as `→ ↗ ↓ ↘`, not digits, which is what the misaki-derived tables in
[`zh.rs`](src/g2p/zh.rs) produce; Japanese needs kanji read through
`jpreprocess`'s dictionary rather than transliterated letter-by-letter.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
kokoro-micro = "1.2"
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
        None,  // voice: None = default "af_sky"
        1.0,   // speed: 1.0 = normal
        1.0,   // gain: 1.0 = normal volume
        None,  // language: taken from the voice
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
  - `lang` - Usually `None`. The language is taken from the voice name; this is
    only consulted for voices that are not recognized as Kokoro voices.

- **`phonemize(text: &str, voice: Option<&str>) -> Result<String, String>`**  
  The phonemes this engine would synthesize, for inspection. Synthesis always
  goes through `synthesize_with_options`, which does this step itself.

- **`save_wav(path: &str, audio: &[f32]) -> Result<(), String>`**  
  Save audio samples to a WAV file.

The `g2p` module exposes `Lang` (with `Lang::from_voice`), `g2p::phonemize` and
`g2p::is_known` for working with phonemes directly.

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

## Listening to a voice

`examples/say.rs` speaks one line and writes a WAV, printing the phonemes it
used - usually the first thing to look at when something sounds wrong. The
language comes from the voice name, so there is nothing else to pass:

```bash
cargo run --release --example say -- zf_xiaoni "你好，世界。我们今天去公园散步，好吗？"
aplay /tmp/say.wav        # or: paplay /tmp/say.wav, ffplay -autoexit /tmp/say.wav

cargo run --release --example say -- jm_kumo "こんにちは。今日はいい天気ですね。" /tmp/ja.wav
aplay /tmp/ja.wav
```

```
voice zf_xiaoni, language z
phonemes: ni↓xau↓, ʂɨ↘ʨje↘. wo↓mən ʨi→ntʰjɛ→n ʨʰy↘ kʊ→ŋɥɛ↗n sa↘npu↘, xau↓ ma?
wrote /tmp/say.wav (7.18s, 24 kHz mono)
```

Two more examples are useful when changing the G2P:

```bash
cargo run --release --example phonemes          # one sample per language, no model needed
cargo run --release --example check_all_voices  # synthesize with all 54 voices
cargo run --release --example edge_cases        # digits, mixed scripts, empty input, ...
```

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

- **`cuda`** - build with ONNX Runtime's CUDA execution provider

```toml
[dependencies]
kokoro-micro = { version = "1.2", features = ["cuda"] }
```

### Choosing a device

Without the `cuda` feature the model runs on the CPU and nothing below
changes anything. With it, `TtsEngine::new()` runs on the GPU when the CUDA
provider comes up and on the CPU when it does not - a failed provider
registration is not an error, and neither is a provider that will not take
the model. To ask for something else, or to be told when the GPU is not there
instead of quietly losing the speedup:

```rust
use kokoro_micro::{Device, TtsEngine};

// `Device::Gpu` fails if CUDA is not compiled in or will not initialize.
// `Device::Auto` (the default) falls back to the CPU instead.
let tts = TtsEngine::new_on_device(Device::Gpu).await?;
println!("running on {}", tts.backend()); // CUDAExecutionProvider (device 0)
```

`Device::parse` accepts `auto`, `cpu`, `gpu` and `gpu:<index>` (with `cuda`
and `cuda:<index>` as aliases), for a `--device` flag or a config file.
`TtsEngine::on_device(model_path, voices_path, device)` is the same thing with
custom model paths.

A card that fills up mid-session fails at inference time, not at load time.
`synthesize_with_options` handles that itself: when a run fails on a GPU it
rebuilds the session on the CPU and retries the utterance once, so the
sentence still comes out and later calls go straight to the CPU.
`TtsEngine::fallback_to_cpu()` and `TtsEngine::reload_on_device(device)` do
the same move on demand.

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

## Regenerating the language tables

The English, Mandarin and Japanese tables under `src/g2p/` are generated and
committed; a normal build never regenerates them. See
[`tools/README.md`](tools/README.md) for how to rebuild them against newer
upstream data.

## Credits

Built with the Kokoro 82M parameter TTS model.
Reduced version from [kokoro-tiny](https://github.com/8b-is/kokoro-tiny) by 8b-is.

The English lexicons (`src/g2p/en_us.tsv`/`en_gb.tsv`) are generated from
[misaki](https://github.com/hexgrad/misaki)'s (Apache-2.0) own pronunciation
dictionaries. The Mandarin/Japanese grapheme-to-phoneme tables are ports of
misaki's `transcription.py` (itself from
[pinyin-to-ipa](https://github.com/stefantaubert/pinyin-to-ipa), MIT) and
`cutlet.py` (from [polm/cutlet](https://github.com/polm/cutlet), MIT), respectively.
Mandarin phrase readings come from [python-pinyin](https://github.com/mozillazg/python-pinyin) (MIT).
