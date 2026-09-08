//! The Kokoro-82M phoneme vocabulary.
//!
//! Taken verbatim from `config.json` in <https://huggingface.co/hexgrad/Kokoro-82M>
//! (114 entries). Two things about it are easy to get wrong and both
//! matter:
//!
//! * The ids are **sparse**. Regenerating them by enumerating a symbol string
//!   produces mostly-right ids with a handful of silent off-by-N errors, and
//!   leaves out tokens that have no plain-IPA spelling at all (`\u{2a6}`,
//!   `\u{2a8}`, `\u{ab67}` for Mandarin affricates, `\u{1d5d}` for the
//!   Japanese compressed vowel, `\u{1d4a}` for the syllabic schwa).
//! * Nothing maps to id 0. It is the boundary token the model expects around
//!   every sequence, which is why [`super::tokenize`] adds it as an integer
//!   rather than as a character.

use std::collections::HashMap;
use std::sync::OnceLock;

static VOCAB_TABLE: &[(char, i64)] = &[
    (';', 1),
    (':', 2),
    (',', 3),
    ('.', 4),
    ('!', 5),
    ('?', 6),
    ('\u{2014}', 9),
    ('\u{2026}', 10),
    ('\"', 11),
    ('(', 12),
    (')', 13),
    ('\u{201c}', 14),
    ('\u{201d}', 15),
    ('\u{20}', 16),  // space
    ('\u{303}', 17),  // combining tilde
    ('\u{2a3}', 18),
    ('\u{2a5}', 19),
    ('\u{2a6}', 20),
    ('\u{2a8}', 21),
    ('\u{1d5d}', 22),
    ('\u{ab67}', 23),
    ('A', 24),
    ('I', 25),
    ('O', 31),
    ('Q', 33),
    ('S', 35),
    ('T', 36),
    ('W', 39),
    ('Y', 41),
    ('\u{1d4a}', 42),
    ('a', 43),
    ('b', 44),
    ('c', 45),
    ('d', 46),
    ('e', 47),
    ('f', 48),
    ('h', 50),
    ('i', 51),
    ('j', 52),
    ('k', 53),
    ('l', 54),
    ('m', 55),
    ('n', 56),
    ('o', 57),
    ('p', 58),
    ('q', 59),
    ('r', 60),
    ('s', 61),
    ('t', 62),
    ('u', 63),
    ('v', 64),
    ('w', 65),
    ('x', 66),
    ('y', 67),
    ('z', 68),
    ('\u{251}', 69),
    ('\u{250}', 70),
    ('\u{252}', 71),
    ('\u{e6}', 72),
    ('\u{3b2}', 75),
    ('\u{254}', 76),
    ('\u{255}', 77),
    ('\u{e7}', 78),
    ('\u{256}', 80),
    ('\u{f0}', 81),
    ('\u{2a4}', 82),
    ('\u{259}', 83),
    ('\u{25a}', 85),
    ('\u{25b}', 86),
    ('\u{25c}', 87),
    ('\u{25f}', 90),
    ('\u{261}', 92),
    ('\u{265}', 99),
    ('\u{268}', 101),
    ('\u{26a}', 102),
    ('\u{29d}', 103),
    ('\u{26f}', 110),
    ('\u{270}', 111),
    ('\u{14b}', 112),
    ('\u{273}', 113),
    ('\u{272}', 114),
    ('\u{274}', 115),
    ('\u{f8}', 116),
    ('\u{278}', 118),
    ('\u{3b8}', 119),
    ('\u{153}', 120),
    ('\u{279}', 123),
    ('\u{27e}', 125),
    ('\u{27b}', 126),
    ('\u{281}', 128),
    ('\u{27d}', 129),
    ('\u{282}', 130),
    ('\u{283}', 131),
    ('\u{288}', 132),
    ('\u{2a7}', 133),
    ('\u{28a}', 135),
    ('\u{28b}', 136),
    ('\u{28c}', 138),
    ('\u{263}', 139),
    ('\u{264}', 140),
    ('\u{3c7}', 142),
    ('\u{28e}', 143),
    ('\u{292}', 147),
    ('\u{294}', 148),
    ('\u{2c8}', 156),
    ('\u{2cc}', 157),
    ('\u{2d0}', 158),
    ('\u{2b0}', 162),
    ('\u{2b2}', 164),
    ('\u{2193}', 169),
    ('\u{2192}', 171),
    ('\u{2197}', 172),
    ('\u{2198}', 173),
    ('\u{1d7b}', 177),
];

/// Highest phoneme count the model can take: `context_length` (512) minus the
/// two boundary tokens.
pub const MAX_PHONEMES: usize = 510;

/// Boundary token placed at both ends of every sequence.
pub const BOUNDARY_TOKEN: i64 = 0;

pub(crate) fn vocab() -> &'static HashMap<char, i64> {
    static VOCAB: OnceLock<HashMap<char, i64>> = OnceLock::new();
    VOCAB.get_or_init(|| VOCAB_TABLE.iter().copied().collect())
}

/// Phonemes -> input ids, wrapped in the boundary token the model expects.
///
/// Characters outside the vocabulary are **dropped**, matching the reference
/// implementations (`kokoro/model.py` filters `None`, Kokoros uses
/// `filter_map`). Mapping them to id 0 instead - as versions up to 1.0.1 did -
/// injects sequence boundaries in the middle of a word, which is audible as
/// dropped syllables and, for tonal languages, destroys the tone marks
/// entirely.
pub(crate) fn tokenize(phonemes: &str) -> Vec<i64> {
    let vocab = vocab();
    let mut tokens = Vec::with_capacity(phonemes.len() + 2);
    tokens.push(BOUNDARY_TOKEN);
    tokens.extend(phonemes.chars().filter_map(|c| vocab.get(&c).copied()));
    tokens.push(BOUNDARY_TOKEN);
    tokens
}

/// Number of phonemes in `phonemes` the model can actually see.
pub(crate) fn phoneme_count(phonemes: &str) -> usize {
    let vocab = vocab();
    phonemes.chars().filter(|c| vocab.contains_key(c)).count()
}
