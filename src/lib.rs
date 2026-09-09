//! kokoro-micro: A minimal, embeddable TTS engine using the Kokoro model
//!
//! This crate provides a simple API for text-to-speech synthesis using the
//! Kokoro 82M parameter model. Perfect for embedding in other applications!
//!
//! # Example
//! ```no_run
//! use kokoro_micro::TtsEngine;
//!
//! #[tokio::main]
//! async fn main() {
//!     // Initialize with auto-download of model if needed
//!     let tts = TtsEngine::new().await.unwrap();
//!
//!     // Generate speech with synthesize_with_options
//!     let audio = tts.synthesize_with_options("Hello world!", None, 1.0, 1.0, None).unwrap();
//!
//!     // Save to file
//!     tts.save_wav("output.wav", &audio).unwrap();
//! }
//! ```
//!
//! # Languages
//!
//! The voice picks the language: `af_heart` is American English, `zf_xiaoni`
//! is Mandarin. Callers do not have to say which, and the `lang` argument is
//! only consulted for voices this crate does not recognize.
//!
//! This matters more than it sounds. Kokoro's nine languages were trained
//! against three different grapheme-to-phoneme front ends, and each speaks a
//! different phoneme alphabet; feeding one language's phonemes to another
//! language's voice is what makes a voice sound subtly wrong rather than
//! obviously broken. [`g2p`] reproduces all three.
//!
//! All nine are built in; there is nothing to enable.
//!
//! # Where it runs
//!
//! On the CPU, unless the crate is built with `--features cuda` and ONNX
//! Runtime's CUDA provider initializes, in which case [`TtsEngine::new`] uses
//! it. [`Device`] chooses explicitly - including requiring a GPU rather than
//! falling back to the CPU - and [`TtsEngine::backend`] reports what was
//! chosen. See [`TtsEngine::on_device`] and [`TtsEngine::fallback_to_cpu`].

mod vocab;

pub mod device;
pub mod g2p;

pub use device::{compiled_gpu_providers, gpu_support_compiled, Backend, Device};
pub use g2p::Lang;

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use ndarray::{ArrayBase, IxDyn, OwnedRepr};
use ndarray_npy::NpzReader;
use ort::{
    session::{Session, SessionInputValue, SessionInputs},
    value::{Tensor, Value},
};

// Debug logging macro - only prints when KOKORO_DEBUG=1
macro_rules! debug_log {
    ($($arg:tt)*) => {
        if std::env::var("KOKORO_DEBUG").map(|v| v == "1").unwrap_or(false) {
            eprintln!($($arg)*);
        }
    };
}

// Constants - Model files stored in GitHub LFS
const MODEL_URL: &str = "https://github.com/8b-is/kokoro-tiny/raw/main/models/0.onnx";
const VOICES_URL: &str = "https://github.com/8b-is/kokoro-tiny/raw/main/models/0.bin";
const SAMPLE_RATE: u32 = 24000; // Kokoro model sample rate
const DEFAULT_VOICE: &str = "af_sky";
const DEFAULT_SPEED: f32 = 1.0; // User-facing normal speed (maps to model 0.65)
const SPEED_SCALE: f32 = 0.65; // Model speed = user speed * this scale factor
const CHUNK_CROSSFADE_MS: usize = 45;
const MIN_ENGINE_SPEED: f32 = 0.35;
/// Prefix on errors raised by the ONNX session itself, as opposed to ones the
/// same call would raise on any device. Only these are retried elsewhere.
const INFERENCE_FAILED_ON: &str = "inference failed on ";
const MAX_ENGINE_SPEED: f32 = 2.2;
/// Longest run of characters we will hand to the model without a place to
/// break: a fallback for scripts that write without spaces or punctuation.
const MAX_CHARS_PER_ATOM: usize = 24;

// Fallback audio message - "Excuse me, I lost my voice. Give me time to get it back."
// This is a pre-generated minimal WAV file that can play while downloading
const FALLBACK_MESSAGE: &[u8] = include_bytes!("../assets/fallback.wav");

// Get cache directory for shared model storage - keeping it minimal like Hue wants!
// Always uses $HOME/.cache/k on all platforms (Windows, macOS, Linux)
fn get_cache_dir() -> PathBuf {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| {
            debug_log!("⚠️  Could not determine HOME directory, using current directory");
            ".".to_string()
        });

    Path::new(&home).join(".cache").join("k")
}

/// Take a lock, ignoring poisoning: every one of these guards a plain field
/// swap, so a panic elsewhere leaves nothing half-written to protect against.
fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Main TTS engine struct
pub struct TtsEngine {
    /// Behind a lock because the engine can move between devices while it is
    /// shared: synthesis clones the inner handle and lets go of the outer one,
    /// so a reload swaps the session without disturbing a run already under
    /// way - that one finishes on the old session, which the clone keeps alive.
    session: Mutex<Option<Arc<Mutex<Session>>>>,
    voices: HashMap<String, Vec<f32>>,
    fallback_mode: bool,
    /// Execution provider the loaded session runs on; the CPU in fallback mode.
    backend: Mutex<Backend>,
    /// Where the model came from, so the session can be rebuilt on another
    /// device without the caller having to remember the path.
    model_path: PathBuf,
}

impl TtsEngine {
    /// Create a new TTS engine, downloading model files if necessary
    ///
    /// Uses `$HOME/.cache/k/` for shared model storage on all platforms:
    /// - Linux/macOS: `$HOME/.cache/k/` (e.g., `/home/user/.cache/k/`)
    /// - Windows: `%USERPROFILE%/.cache/k/` (e.g., `C:\Users\Username\.cache\k\`)
    pub async fn new() -> Result<Self, String> {
        Self::new_on_device(Device::Auto).await
    }

    /// Same as [`Self::new`] with an explicit [`Device`].
    pub async fn new_on_device(device: Device) -> Result<Self, String> {
        let cache_dir = get_cache_dir();
        let model_path = cache_dir.join("0.onnx");
        let voices_path = cache_dir.join("0.bin");

        Self::on_device(
            model_path.to_str().unwrap_or("0.onnx"),
            voices_path.to_str().unwrap_or("0.bin"),
            device,
        )
        .await
    }

    /// Create a new TTS engine with custom model paths
    pub async fn with_paths(model_path: &str, voices_path: &str) -> Result<Self, String> {
        Self::on_device(model_path, voices_path, Device::Auto).await
    }

    /// Create a new TTS engine with custom model paths, on an explicit
    /// [`Device`].
    ///
    /// [`Device::Auto`] - the default everywhere else - runs on the GPU when
    /// this crate was built with `--features cuda` and the provider comes up,
    /// and on the CPU otherwise; it never fails because of the GPU.
    /// [`Device::Gpu`] and [`Device::GpuIndex`] fail instead of falling back,
    /// for callers who would rather hear about a broken CUDA install than
    /// quietly run 20x slower. [`Self::backend`] reports what was chosen.
    ///
    /// The device is resolved before anything is downloaded, so an impossible
    /// request comes back immediately rather than after 337MB.
    pub async fn on_device(
        model_path: &str,
        voices_path: &str,
        device: Device,
    ) -> Result<Self, String> {
        // Cheap and early: a GPU asked of a build that has none is hopeless
        // whatever the machine holds, and saying so now beats saying it after
        // a 337MB download. Which provider actually gets the model is settled
        // once, by load_session below.
        device::check_device(device)?;

        // Ensure cache directory exists
        if let Some(parent) = Path::new(model_path).parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create cache directory: {}", e))?;
        }

        // Check if we need to download
        let need_download = !Path::new(model_path).exists() || !Path::new(voices_path).exists();

        if need_download {
            debug_log!("🎤 First time setup - downloading voice model...");
            debug_log!("   (This only happens once, files will be cached in ~/.cache/k)");

            // Try to download the files
            let download_success = {
                let mut success = true;

                // Download model if needed
                if !Path::new(model_path).exists() {
                    debug_log!("   📥 Downloading model (310MB)...");
                    if let Err(e) = download_file(MODEL_URL, model_path).await {
                        debug_log!("   ❌ Failed to download model: {}", e);
                        success = false;
                    }
                }

                // Download voices if needed
                if success && !Path::new(voices_path).exists() {
                    debug_log!("   📥 Downloading voices (27MB)...");
                    if let Err(e) = download_file(VOICES_URL, voices_path).await {
                        debug_log!("   ❌ Failed to download voices: {}", e);
                        success = false;
                    }
                }

                if success {
                    debug_log!("   ✅ Voice model downloaded successfully!");
                }

                success
            };

            // If download failed, return fallback engine
            if !download_success {
                debug_log!("\n⚠️  Using fallback mode. The model files are not available at:");
                debug_log!("   - {}", MODEL_URL);
                debug_log!("   - {}", VOICES_URL);
                debug_log!("\n💡 Please manually download the model files to ~/.cache/k/");

                return Ok(Self {
                    session: Mutex::new(None),
                    voices: HashMap::new(),
                    fallback_mode: true,
                    backend: Mutex::new(Backend::cpu()),
                    model_path: PathBuf::from(model_path),
                });
            }
        }

        // Load ONNX model
        let (session, backend) = device::load_session(model_path, device)?;
        debug_log!("🧠 Model loaded on {}", backend);

        // Load voices
        let voices = load_voices(voices_path)?;

        Ok(Self {
            session: Mutex::new(Some(Arc::new(Mutex::new(session)))),
            voices,
            fallback_mode: false,
            backend: Mutex::new(backend),
            model_path: PathBuf::from(model_path),
        })
    }

    /// The execution provider this engine runs on: `CPU`, or
    /// `CUDAExecutionProvider` and the device index. Cloned rather than
    /// borrowed because it changes when the engine moves devices.
    pub fn backend(&self) -> Backend {
        lock(&self.backend).clone()
    }

    /// Rebuild the session on another [`Device`], keeping the loaded voices.
    ///
    /// Load-time device selection cannot cover the case that matters most in
    /// a long-lived process: a card that fills up - or is taken away - while
    /// the engine is alive. Inference then fails at run time, and the only
    /// way back is a new session. The old one is dropped first so its VRAM is
    /// released before the new one asks for any, which also means a failed
    /// reload leaves the engine without a session: call it again.
    ///
    /// Takes `&self`: the engine is normally shared, and demanding `&mut` for
    /// something that happens once would push a lock onto every caller.
    pub fn reload_on_device(&self, device: Device) -> Result<(), String> {
        if self.fallback_mode {
            return Err("TTS engine is in fallback mode; there is no model to reload".to_string());
        }
        device::check_device(device)?;
        // Only the CPU can be recognised as "already there" without loading:
        // which GPU provider a device resolves to is decided by the load.
        if !device.wants_gpu() && !lock(&self.backend).is_gpu() && lock(&self.session).is_some() {
            return Ok(());
        }

        // Drop the old session before building the new one: when the reload
        // is happening because the card filled up, that VRAM has to come back
        // before the new session asks for any. A synthesis already running
        // holds its own handle and finishes on the old session.
        *lock(&self.session) = None;

        // On failure the engine is left without a session and `backend` still
        // names the device the dropped one ran on - another reload is the only
        // way back either way.
        let (session, backend) = device::load_session(&self.model_path, device)?;
        debug_log!("🔁 Model reloaded on {}", backend);
        *lock(&self.session) = Some(Arc::new(Mutex::new(session)));
        *lock(&self.backend) = backend;
        Ok(())
    }

    /// Rebuild the session on the CPU. A no-op when it already runs there.
    ///
    /// [`Self::synthesize_with_options`] calls this itself when inference
    /// fails on a GPU, so most callers never need it; reach for it directly
    /// to give the card back before some other process needs it.
    pub fn fallback_to_cpu(&self) -> Result<(), String> {
        if !lock(&self.backend).is_gpu() {
            return Ok(());
        }
        self.reload_on_device(Device::Cpu)
    }

    /// List all available voices
    pub fn voices(&self) -> Vec<String> {
        if self.fallback_mode {
            vec!["fallback".to_string()]
        } else {
            self.voices.keys().cloned().collect()
        }
    }

    /// Synthesize text to speech with full options
    ///
    /// Speed: 0.5 = half speed (slower), 1.0 = normal, 2.0 = double speed (faster)
    /// Gain: 0.5 = quieter, 1.0 = normal, 2.0 = twice as loud (with soft clipping)
    ///
    /// The language comes from the voice - `af_heart` is American English,
    /// `zf_xiaoni` is Mandarin - so `lang` is only consulted for voice names
    /// this crate does not recognize. Pass `None` unless you have such a voice.
    pub fn synthesize_with_options(
        &self,
        text: &str,
        voice: Option<&str>,
        speed: f32,
        gain: f32,
        lang: Option<&str>,
    ) -> Result<Vec<f32>, String> {
        // If in fallback mode, return the excuse message audio
        if self.fallback_mode {
            debug_log!("🎤 Playing fallback message while downloading voice model...");
            return wav_to_f32(FALLBACK_MESSAGE);
        }

        let voice = voice.unwrap_or(DEFAULT_VOICE);
        let lang = self.resolve_lang(voice, lang);
        let chunks = phonemize_chunks(text, lang)?;
        if chunks.is_empty() {
            return Err("No text provided for synthesis".to_string());
        }

        debug_log!(
            "📚 {} chars -> {} phoneme chunk(s) [{}]",
            text.chars().count(),
            chunks.len(),
            lang.code()
        );

        // Read before the match rather than in its guard: the arm below takes
        // the same lock again, and std mutexes are not reentrant.
        let on_gpu = self.backend().is_gpu();
        match self.synthesize_phoneme_chunks(&chunks, voice, speed, gain) {
            Err(e) if on_gpu && e.starts_with(INFERENCE_FAILED_ON) => {
                // A GPU that runs out of memory mid-session fails here, not at
                // load time. Rebuild on the CPU and say the sentence rather
                // than lose it; every later call goes to the CPU too.
                debug_log!("⚠️  {} - falling back to CPU", e);
                self.fallback_to_cpu()
                    .map_err(|rebuild| format!("{} (CPU fallback also failed: {})", e, rebuild))?;
                self.synthesize_phoneme_chunks(&chunks, voice, speed, gain)
            }
            result => result,
        }
    }

    /// The phonemes this engine would synthesize for `text` in `voice`.
    ///
    /// Exposed for inspection; synthesis always goes through
    /// [`Self::synthesize_with_options`], which does this step itself.
    pub fn phonemize(&self, text: &str, voice: Option<&str>) -> Result<String, String> {
        let voice = voice.unwrap_or(DEFAULT_VOICE);
        g2p::phonemize(text, self.resolve_lang(voice, None))
    }

    /// Which language a request is in: the voice knows, so it wins.
    fn resolve_lang(&self, voice: &str, lang: Option<&str>) -> Lang {
        Lang::from_voice(voice)
            .or_else(|| lang.and_then(Lang::from_name))
            .unwrap_or(Lang::AmericanEnglish)
    }

    fn synthesize_phoneme_chunks(
        &self,
        chunks: &[String],
        voice: &str,
        speed: f32,
        gain: f32,
    ) -> Result<Vec<f32>, String> {
        // Cloned out of the lock so inference does not hold it: a reload can
        // then swap the session while this run finishes on the old one.
        let session = lock(&self.session)
            .as_ref()
            .cloned()
            .ok_or_else(|| "TTS engine not initialized".to_string())?;

        // Map user-facing speed to model speed (user 1.0 = model 0.65)
        let clamped_speed = (speed * SPEED_SCALE).clamp(MIN_ENGINE_SPEED, MAX_ENGINE_SPEED);
        let overlap = chunk_crossfade_samples();
        let mut combined = Vec::new();

        for (idx, chunk) in chunks.iter().enumerate() {
            debug_log!(
                "   → Chunk {}/{} ({} phonemes)",
                idx + 1,
                chunks.len(),
                vocab::phoneme_count(chunk)
            );
            let audio = self.synthesize_segment(&session, voice, chunk, clamped_speed)?;
            append_with_crossfade(&mut combined, &audio, overlap);
        }

        if combined.is_empty() {
            return Err("Failed to synthesize combined audio".to_string());
        }
        if gain != 1.0 {
            combined = amplify_audio(&combined, gain);
        }
        Ok(combined)
    }

    fn synthesize_segment(
        &self,
        session: &Arc<Mutex<Session>>,
        voice: &str,
        phonemes: &str,
        speed: f32,
    ) -> Result<Vec<f32>, String> {
        let count = vocab::phoneme_count(phonemes);
        if count == 0 {
            return Ok(Vec::new());
        }
        // The model's positional embeddings stop at `context_length`; past it
        // ONNX Runtime fails deep inside the encoder with a shape error that
        // tells the caller nothing. Chunking upstream should mean we never get
        // here, so say plainly what happened rather than let it through.
        if count > vocab::MAX_PHONEMES {
            return Err(format!(
                "segment is {} phonemes, model limit is {}",
                count,
                vocab::MAX_PHONEMES
            ));
        }

        let tokens = vocab::tokenize(phonemes);

        // Kokoro voice files are [max_seq_len, 1, 256] tables and the model
        // wants the row matching this utterance's length, indexed by phoneme
        // count excluding the boundary tokens (reference: `pack[len(ps)-1]`).
        let style = self.parse_voice_style(voice, count.saturating_sub(1))?;

        // Tagged, because only a failure inside the session is worth rebuilding
        // the engine on another device for: an unknown voice or an over-long
        // segment fails the same way wherever the model runs.
        self.run_inference(&session, tokens, style, speed)
            .map_err(|e| format!("{}{}: {}", INFERENCE_FAILED_ON, self.backend(), e))
    }

    /// Save audio as WAV file
    pub fn save_wav(&self, path: &str, audio: &[f32]) -> Result<(), String> {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };

        let mut writer = hound::WavWriter::create(path, spec)
            .map_err(|e| format!("Failed to create WAV file: {}", e))?;

        for &sample in audio {
            let sample_i16 = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
            writer
                .write_sample(sample_i16)
                .map_err(|e| format!("Failed to write sample: {}", e))?;
        }

        writer
            .finalize()
            .map_err(|e| format!("Failed to finalize WAV: {}", e))?;
        Ok(())
    }

    // Private helper methods

    fn parse_voice_style(&self, voice_str: &str, style_row: usize) -> Result<Vec<f32>, String> {
        if self.fallback_mode {
            // Return a dummy style vector for fallback mode
            return Ok(vec![0.0; 256]);
        }

        let mut result = vec![0.0; 256];
        let parts: Vec<&str> = voice_str.split('+').collect();

        for part in parts {
            let (voice_name, weight) = if part.contains('.') {
                let pieces: Vec<&str> = part.split('.').collect();
                if pieces.len() != 2 {
                    return Err(format!("Invalid voice format: {}", part));
                }
                let weight = pieces[1]
                    .parse::<f32>()
                    .map_err(|_| format!("Invalid weight: {}", pieces[1]))?;
                (pieces[0], weight / 10.0)
            } else {
                (part, 1.0)
            };

            let voice_style = self
                .voices
                .get(voice_name)
                .ok_or_else(|| format!("Voice not found: {}", voice_name))?;

            // Kokoro voice files (0.bin) are stored as [max_seq_len, 1, 256]
            // f32 tables; load_voices flattens that into a single Vec<f32>.
            // The model needs the row matching this utterance's length, so
            // pick the [style_row*256 .. style_row*256+256] slice. Taking
            // [0..256] - the style for an empty sentence - for every input is
            // what caused the "first second is clear, the rest is murmurs"
            // dropout. Clamp so very long inputs re-use the last stored row.
            let style_dim: usize = 256;
            let max_idx = voice_style.len().saturating_sub(style_dim) / style_dim;
            let idx = style_row.min(max_idx);
            let offset = idx * style_dim;
            let slice_end = (offset + style_dim).min(voice_style.len());
            for (i, val) in voice_style[offset..slice_end].iter().enumerate() {
                if i < result.len() {
                    result[i] += val * weight;
                }
            }
        }

        Ok(result)
    }

    fn run_inference(
        &self,
        session: &Arc<Mutex<Session>>,
        tokens: Vec<i64>,
        style: Vec<f32>,
        speed: f32,
    ) -> Result<Vec<f32>, String> {
        let mut session = session
            .lock()
            .map_err(|e| format!("Failed to lock session: {}", e))?;

        let token_count = tokens.len(); // Save count before moving

        // Prepare tokens tensor
        let tokens_array = ndarray::Array2::from_shape_vec((1, tokens.len()), tokens)
            .map_err(|e| format!("Failed to create tokens array: {}", e))?;
        let tokens_tensor = Tensor::from_array(tokens_array)
            .map_err(|e| format!("Failed to create tokens tensor: {}", e))?;

        // Prepare style tensor
        let style_array = ndarray::Array2::from_shape_vec((1, style.len()), style)
            .map_err(|e| format!("Failed to create style array: {}", e))?;
        let style_tensor = Tensor::from_array(style_array)
            .map_err(|e| format!("Failed to create style tensor: {}", e))?;

        // Prepare speed tensor
        let speed_array = ndarray::Array1::from_vec(vec![speed]);
        let speed_tensor = Tensor::from_array(speed_array)
            .map_err(|e| format!("Failed to create speed tensor: {}", e))?;

        // Create inputs
        use std::borrow::Cow;
        let inputs = SessionInputs::from(vec![
            (
                Cow::Borrowed("tokens"),
                SessionInputValue::Owned(Value::from(tokens_tensor)),
            ),
            (
                Cow::Borrowed("style"),
                SessionInputValue::Owned(Value::from(style_tensor)),
            ),
            (
                Cow::Borrowed("speed"),
                SessionInputValue::Owned(Value::from(speed_tensor)),
            ),
        ]);

        // Run inference
        let outputs = session
            .run(inputs)
            .map_err(|e| format!("Failed to run inference: {}", e))?;

        // Extract audio
        let (shape, data) = outputs["audio"]
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("Failed to extract audio tensor: {}", e))?;

        // Debug output shape for longer text
        let data_vec = data.to_vec();
        if token_count > 100 {
            debug_log!(
                "   Output audio shape: {:?}, samples: {}",
                shape,
                data_vec.len()
            );
        }

        Ok(data_vec)
    }
}

// Helper functions

// Load voices from binary file
fn load_voices(path: &str) -> Result<HashMap<String, Vec<f32>>, String> {
    let mut file = File::open(path).map_err(|e| format!("Failed to open voices file: {}", e))?;

    let mut reader =
        NpzReader::new(&mut file).map_err(|e| format!("Failed to create NPZ reader: {}", e))?;

    let mut voices = HashMap::new();

    for name in reader
        .names()
        .map_err(|e| format!("Failed to read NPZ names: {:?}", e))?
    {
        let array: ArrayBase<OwnedRepr<f32>, IxDyn> = reader
            .by_name(&name)
            .map_err(|e| format!("Failed to read NPZ array {}: {:?}", name, e))?;
        let data: Vec<f32> = array.iter().cloned().collect();

        // Clean up the name (remove .npy extension if present)
        let clean_name = name.trim_end_matches(".npy");
        voices.insert(clean_name.to_string(), data);
    }

    Ok(voices)
}

// Download file from URL
async fn download_file(url: &str, path: &str) -> Result<(), Box<dyn std::error::Error>> {
    let response = reqwest::get(url).await?;
    let bytes = response.bytes().await?;

    let mut file = File::create(path)?;
    file.write_all(&bytes)?;

    Ok(())
}

// Convert WAV bytes to f32 samples
fn wav_to_f32(wav_bytes: &[u8]) -> Result<Vec<f32>, String> {
    let cursor = Cursor::new(wav_bytes);
    let mut reader =
        hound::WavReader::new(cursor).map_err(|e| format!("Failed to read WAV: {}", e))?;

    let samples: Result<Vec<f32>, _> = reader
        .samples::<i16>()
        .map(|s| s.map(|sample| sample as f32 / 32768.0))
        .collect();

    samples.map_err(|e| format!("Failed to read samples: {}", e))
}

fn chunk_crossfade_samples() -> usize {
    ((SAMPLE_RATE as usize) * CHUNK_CROSSFADE_MS) / 1000
}

fn append_with_crossfade(buffer: &mut Vec<f32>, next: &[f32], overlap_samples: usize) {
    if next.is_empty() {
        return;
    }

    if buffer.is_empty() || overlap_samples == 0 {
        buffer.extend_from_slice(next);
        return;
    }

    let overlap = overlap_samples.min(buffer.len()).min(next.len());
    if overlap == 0 {
        buffer.extend_from_slice(next);
        return;
    }

    let start = buffer.len() - overlap;
    for i in 0..overlap {
        let fade_in = i as f32 / overlap as f32;
        let fade_out = 1.0 - fade_in;
        buffer[start + i] = buffer[start + i] * fade_out + next[i] * fade_in;
    }

    buffer.extend_from_slice(&next[overlap..]);
}

// --- chunking --------------------------------------------------------------
//
// The model takes at most `vocab::MAX_PHONEMES` per pass, so long input has to
// be broken up. Splitting on a character budget - as this crate used to - is
// wrong twice over: how many phonemes a character becomes varies by an order
// of magnitude between languages, and the split points themselves (ASCII `.`
// and whitespace) do not exist in Chinese or Japanese text, so CJK input was
// never split at all and simply failed in the ONNX session.
//
// Instead: split on real sentence boundaries in any script, phonemize, and
// pack the results back up to the budget.

/// Sentence-ending punctuation, in every script the model supports.
fn is_sentence_end(c: char) -> bool {
    matches!(c, '.' | '!' | '?' | '\u{3002}' | '\u{ff01}' | '\u{ff1f}' | '\u{2026}')
}

/// Clause-separating punctuation, likewise.
fn is_clause_end(c: char) -> bool {
    matches!(c, ',' | ';' | ':' | '\u{ff0c}' | '\u{3001}' | '\u{ff1b}' | '\u{ff1a}' | '\u{2014}')
}

/// Split after each character matching `at`, keeping it with its sentence.
fn split_keeping(text: &str, at: fn(char) -> bool) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        current.push(c);
        if at(c) && !current.trim().is_empty() {
            parts.push(std::mem::take(&mut current));
        }
    }
    if !current.trim().is_empty() {
        parts.push(current);
    }
    parts
        .into_iter()
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

/// Last-resort split for text with no punctuation and no spaces.
fn split_atoms(text: &str) -> Vec<String> {
    if text.split_whitespace().count() > 1 {
        return text.split_whitespace().map(str::to_string).collect();
    }
    let chars: Vec<char> = text.chars().collect();
    chars
        .chunks(MAX_CHARS_PER_ATOM)
        .map(|c| c.iter().collect())
        .collect()
}

/// Append `phonemes` to `chunks`, merging into the previous chunk while it
/// still fits so short sentences share one pass and keep their pacing.
fn push_packed(chunks: &mut Vec<String>, phonemes: String) {
    if phonemes.trim().is_empty() {
        return;
    }
    if let Some(last) = chunks.last_mut() {
        if vocab::phoneme_count(last) + 1 + vocab::phoneme_count(&phonemes)
            <= vocab::MAX_PHONEMES
        {
            last.push(' ');
            last.push_str(phonemes.trim());
            return;
        }
    }
    chunks.push(phonemes.trim().to_string());
}

/// Text -> phoneme chunks, each within the model's context length.
fn phonemize_chunks(text: &str, lang: Lang) -> Result<Vec<String>, String> {
    let mut chunks = Vec::new();
    for line in text.split('\n') {
        if line.trim().is_empty() {
            continue;
        }
        add_phonemized(line, lang, 0, &mut chunks)?;
    }
    Ok(chunks)
}

fn add_phonemized(
    text: &str,
    lang: Lang,
    depth: usize,
    chunks: &mut Vec<String>,
) -> Result<(), String> {
    let phonemes = g2p::phonemize(text, lang)?;
    if vocab::phoneme_count(&phonemes) <= vocab::MAX_PHONEMES {
        push_packed(chunks, phonemes);
        return Ok(());
    }

    // Try progressively finer split points, taking the first that actually
    // divides the text. Falling straight through to a truncation when the
    // coarsest one finds nothing would lose the tail of any long single
    // sentence - text with commas but no full stop is entirely ordinary.
    let splitters: [fn(&str) -> Vec<String>; 3] = [
        |t| split_keeping(t, is_sentence_end),
        |t| split_keeping(t, is_clause_end),
        split_atoms,
    ];
    let parts = splitters
        .iter()
        .skip(depth)
        .map(|split| split(text))
        .find(|parts| parts.len() > 1)
        .unwrap_or_default();

    // No split point left at any granularity: keep what fits rather than
    // failing the whole utterance, and say so when debugging is on.
    if parts.len() < 2 {
        debug_log!(
            "   ⚠️  {} phonemes with nowhere to split; truncating to {}",
            vocab::phoneme_count(&phonemes),
            vocab::MAX_PHONEMES
        );
        let truncated: String = phonemes.chars().take(vocab::MAX_PHONEMES).collect();
        push_packed(chunks, truncated);
        return Ok(());
    }

    for part in parts {
        add_phonemized(&part, lang, depth + 1, chunks)?;
    }
    Ok(())
}

// Amplify audio - allows some clipping for maximum loudness
fn amplify_audio(audio: &[f32], gain: f32) -> Vec<f32> {
    audio
        .iter()
        .map(|&sample| {
            let amplified = sample * gain;

            // Simple hard clipping at the limits
            // This allows maximum volume even if it distorts a bit
            amplified.clamp(-1.0, 1.0)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crossfade_extends_buffer() {
        let mut buffer = vec![1.0, 1.0, 1.0];
        let next = vec![0.0, 0.0, 0.0];
        append_with_crossfade(&mut buffer, &next, 2);
        // Result should be len 4 (3 + 3 - overlap)
        assert_eq!(buffer.len(), 4);
        // Last sample should come from next chunk
        assert!((buffer.last().copied().unwrap() - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn short_text_stays_in_one_chunk() {
        let chunks = phonemize_chunks("Hello world.", Lang::AmericanEnglish).unwrap();
        assert_eq!(chunks.len(), 1);
        assert!(vocab::phoneme_count(&chunks[0]) <= vocab::MAX_PHONEMES);
    }

    #[test]
    fn every_chunk_fits_the_model_context() {
        let long = "This sentence is deliberately repetitive. ".repeat(80);
        let chunks = phonemize_chunks(&long, Lang::AmericanEnglish).unwrap();
        assert!(chunks.len() > 1, "long input should be split");
        for chunk in &chunks {
            assert!(
                vocab::phoneme_count(chunk) <= vocab::MAX_PHONEMES,
                "chunk of {} phonemes exceeds the {} limit",
                vocab::phoneme_count(chunk),
                vocab::MAX_PHONEMES
            );
        }
    }

    #[test]
    fn splits_on_sentence_boundaries_in_any_script() {
        assert_eq!(split_keeping("A. B. C.", is_sentence_end).len(), 3);
        // No ASCII punctuation and no spaces: the old chunker never split this.
        let zh = split_keeping("\u{4f60}\u{597d}\u{3002}\u{4e16}\u{754c}\u{3002}", is_sentence_end);
        assert_eq!(zh.len(), 2, "{zh:?}");
    }

    #[test]
    fn a_long_sentence_with_no_full_stop_is_not_truncated() {
        // Commas but no sentence terminator: the coarsest splitter finds
        // nothing, and we must fall through to the finer ones rather than
        // throwing the tail away.
        let long = "one thing, and another thing, ".repeat(60);
        let chunks = phonemize_chunks(&long, Lang::AmericanEnglish).unwrap();
        let kept: usize = chunks.iter().map(|c| vocab::phoneme_count(c)).sum();
        let whole = vocab::phoneme_count(&g2p::phonemize(&long, Lang::AmericanEnglish).unwrap());
        assert!(chunks.len() > 1, "should have been split");
        assert!(
            kept as f32 > whole as f32 * 0.95,
            "kept {kept} of {whole} phonemes - the tail was dropped"
        );
    }

    #[test]
    fn text_without_any_break_still_gets_split() {
        let runon = "a".repeat(500);
        assert!(split_atoms(&runon).len() > 1);
    }
}
