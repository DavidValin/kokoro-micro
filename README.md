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
| `a` | `af_* am_*` (20) | eSpeak NG `en-us` + misaki rewrites |
| `b` | `bf_* bm_*` (8) | eSpeak NG `en-gb` + misaki rewrites |
| `e` `f` `h` `i` `p` | `ef_ em_ ff_ hf_ hm_ if_ im_ pf_ pm_` (13) | eSpeak NG, one voice per language |
| `z` | `zf_* zm_*` (8) | jieba + pinyin + misaki's transcription tables |
| `j` | `jf_* jm_*` (5) | OpenJTalk (`jpreprocess`) + misaki's kana table |

This is not incidental detail. Kokoro-82M was trained against three different
grapheme-to-phoneme front ends, each speaking a different phoneme alphabet, and
the model's vocabulary is only 114 tokens wide. Anything outside it is dropped
before inference, so a language handled by the wrong front end does not fail
loudly - it comes back mumbling. Two examples of what that looked like before
this crate grew per-language front ends:

- eSpeak's Mandarin voice writes tones as **digits** (`hˈɑu2`), and the model has
  no digit tokens. Every tone in the sentence was dropped. Mandarin needs
  `→ ↗ ↓ ↘`, which only the misaki tables produce.
- eSpeak cannot read kanji. It fell back to spelling them out *in English*, so
  今日 came out as the English words "chinese letter".

### eSpeak NG data

The eSpeak-backed languages need `espeak-ng-data`. It is looked for, in order,
in `$PIPER_ESPEAKNG_DATA_DIRECTORY`, `$ESPEAK_DATA_PATH`, the current directory,
next to the executable (and one level up, so `target/release/examples/foo`
finds `target/release`), the directory the build installed it to, and finally
the usual system prefixes - `/usr/share`, `/usr/local/share`, `/opt/homebrew/share`
and friends. Each is checked for a real `espeak-ng-data/phontab` before being
used, and the search continues past any that does not have one; the environment
variables are a hint about where to look first, not a hard override. A system
install of espeak-ng therefore needs no configuration at all.

Do not rely on espeak-ng's own built-in default path. `espeak-rs-sys` compiles
in the `OUT_DIR` of whichever build produced the library, which is a path under
`target/` that will not survive a `cargo clean`, a dependency bump, or moving
the binary to another machine - and when it goes stale espeak prints a bare
"No such file or directory" for `phontab` and then fails to initialize.

One caveat if you ship your own copy of the data: it must come from the same
espeak-ng release as the linked library. Mixing versions does not error - most
languages keep working - but the Mandarin tables in particular can decode to
nonsense. Mandarin no longer goes through eSpeak here, so that mismatch can no
longer affect it, but it is still worth keeping in sync.

## Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
kokoro-micro = "1.1"
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

- **`cuda`** - Enable CUDA acceleration for ONNX Runtime

```toml
[dependencies]
kokoro-micro = { version = "1.1", features = ["cuda"] }
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

## Regenerating the language tables

The Mandarin and Japanese tables under `src/g2p/` are generated and committed;
a normal build never regenerates them. See [`tools/README.md`](tools/README.md)
for how to rebuild them against newer upstream data.

## Credits

Built with the Kokoro 82M parameter TTS model.
Reduced version from [kokoro-tiny](https://github.com/8b-is/kokoro-tiny) by 8b-is.

The grapheme-to-phoneme tables are ports of [misaki](https://github.com/hexgrad/misaki)
(Apache-2.0), Kokoro's own G2P: `espeak.py` for the eSpeak-backed languages,
`transcription.py` for Mandarin (itself from [pinyin-to-ipa](https://github.com/stefantaubert/pinyin-to-ipa), MIT),
and `cutlet.py` for Japanese (from [polm/cutlet](https://github.com/polm/cutlet), MIT).
Mandarin phrase readings come from [python-pinyin](https://github.com/mozillazg/python-pinyin) (MIT).
