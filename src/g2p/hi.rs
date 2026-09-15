//! Hindi (Devanagari) grapheme-to-phoneme.
//!
//! Devanagari is an abugida: every consonant letter carries an inherent
//! vowel (/ə/, "schwa") unless followed by a vowel sign (a "matra") or a
//! virama (्, which suppresses it for consonant clusters). The central
//! difficulty this creates: that inherent schwa is very often *not*
//! pronounced, both word-finally (राम is spoken "rām", not "rāma") and
//! word-medially (नमकीन is "namkīn", not "namakīna") - and nothing in the
//! spelling marks which schwas survive.
//!
//! Word-medial deletion is implemented here following the rule in Ohala
//! (1983), as formalized with worked examples in Narasimhan, Sproat & Kiraz,
//! *Schwa-Deletion in Hindi Text-to-Speech Synthesis* (Int. J. Speech
//! Technology 7, 2004): scanning the word right-to-left, a schwa deletes
//! when it is immediately preceded by a vowel then one or two consonants
//! (V-C or V-C-C) *and* immediately followed by a single consonant then a
//! vowel (C-V). Their paper's examples (in their transliteration): "malaatii"
//! (V-CəC-V) -> "maalt(ii)"; "mazaabuut" (V-CəC-V) -> "mazb(uu)t"; "jangal"
//! (V-CCəC-V) -> "jangl". [`resolve_schwas`] implements exactly this,
//! including the right-to-left evaluation order, which matters: deleting a
//! schwa changes the consonant-cluster context that the next (leftward)
//! schwa's own deletion check sees. This naturally reproduces known
//! exceptions without special-casing them - e.g. कमल ("kamal") keeps *both*
//! its non-final schwas, because deleting the last one already removes the
//! only vowel that could ever complete a "C-V" to its left.
//!
//! Anusvara (ं) is also resolved contextually rather than always nasalizing
//! the vowel before it: before one of the 20 stop consonants it assimilates
//! to that consonant's own place of articulation as a real nasal segment
//! (गंगा is [ɡəŋɡa], not a nasalized vowel before plain /ɡ/) - see
//! [`homorganic_nasal`], following Bhaskararao & Mathew (1992)'s "anusvAra
//! conversion" step as summarized in the Narasimhan/Sproat/Kiraz paper above.
//! Elsewhere (before a vowel, a non-stop consonant, or word-finally) it
//! nasalizes the vowel, same as chandrabindu (ँ) always does.
//!
//! What is not attempted: the paper's morpheme-boundary exceptions (a schwa
//! at a prefix/suffix boundary sometimes resists deletion that the bare
//! phonological rule would otherwise apply - detecting that needs
//! morphological analysis this crate does not have) and the general
//! phonotactic filter on which resulting consonant clusters are legal (a
//! rare medial deletion here could produce a cluster real Hindi would not).
//! Also not attempted: nuqta consonants (ड़/ढ़/... for loanword sounds not
//! native to Sanskrit-derived Hindi) fall back to their non-nuqta base
//! letter, and no lexical stress is marked - Hindi word stress is weak and
//! largely predictable from syllable weight rather than contrastive the way
//! English's is, so the other languages' approach of marking one syllable
//! primary would be more noise than signal here.

use super::text;

/// One resolved or to-be-resolved sound in a word, left to right.
#[derive(Clone, Debug, PartialEq)]
enum Unit {
    /// A vowel from an independent vowel letter or a matra - never a
    /// deletion candidate, unlike `Schwa`.
    Vowel(String),
    /// A bare consonant's inherent /ə/: still undecided until
    /// [`resolve_schwas`] runs. `nasal` records an anusvara/chandrabindu
    /// that nasalizes it (as opposed to one that assimilated into its own
    /// consonant unit instead - see [`Nasal`]), applied if the schwa
    /// survives.
    Schwa { nasal: bool },
    /// A consonant sound with no vowel of its own (from a virama cluster, an
    /// assimilated anusvara, or a schwa that [`resolve_schwas`] deleted).
    Consonant(String),
}

/// Independent vowels (used word-initially or after another vowel) and
/// dependent vowel signs (matras, used after a consonant) share a sound
/// inventory; matras are looked up in a separate table only because they are
/// different Unicode code points from their independent-vowel counterparts.
fn independent_vowel(c: char) -> Option<&'static str> {
    Some(match c {
        'अ' => "\u{259}", // ə
        'आ' => "a",
        'इ' => "\u{26a}", // ɪ
        'ई' => "i",
        'उ' => "\u{28a}", // ʊ
        'ऊ' => "u",
        'ऋ' => "r\u{26a}",
        'ए' => "e",
        'ऐ' => "\u{25b}", // ɛ
        'ओ' => "o",
        'औ' => "\u{254}", // ɔ
        _ => return None,
    })
}

fn matra(c: char) -> Option<&'static str> {
    Some(match c {
        '\u{93e}' => "a",          // ा
        '\u{93f}' => "\u{26a}",    // ि
        '\u{940}' => "i",          // ी
        '\u{941}' => "\u{28a}",    // ु
        '\u{942}' => "u",          // ू
        '\u{943}' => "r\u{26a}",   // ृ
        '\u{947}' => "e",          // े
        '\u{948}' => "\u{25b}",    // ै
        '\u{94b}' => "o",          // ो
        '\u{94c}' => "\u{254}",    // ौ
        _ => return None,
    })
}

fn consonant(c: char) -> Option<&'static str> {
    Some(match c {
        'क' => "k",
        'ख' => "k\u{2b0}",
        'ग' => "\u{261}",
        'घ' => "\u{261}\u{2b0}",
        'ङ' => "\u{14b}",
        'च' => "\u{2a7}",
        'छ' => "\u{2a7}\u{2b0}",
        'ज' => "\u{2a4}",
        'झ' => "\u{2a4}\u{2b0}",
        'ञ' => "\u{272}",
        'ट' => "\u{288}",
        'ठ' => "\u{288}\u{2b0}",
        'ड' => "\u{256}",
        'ढ' => "\u{256}\u{2b0}",
        'ण' => "\u{273}",
        'त' => "t",
        'थ' => "t\u{2b0}",
        'द' => "d",
        'ध' => "d\u{2b0}",
        'न' => "n",
        'प' => "p",
        'फ' => "p\u{2b0}",
        'ब' => "b",
        'भ' => "b\u{2b0}",
        'म' => "m",
        'य' => "j",
        'र' => "\u{27e}", // ɾ
        'ल' => "l",
        'व' => "\u{28b}", // ʋ
        'श' => "\u{283}", // ʃ
        'ष' => "\u{282}", // ʂ
        'स' => "s",
        'ह' => "h",
        // Nuqta consonants (ड़/ढ़/फ़/ज़/क़/ग़, for loanword sounds) are the
        // base letter here plus a combining nuqta (U+093C) - two codepoints,
        // not one, so they can't be `char` match arms. The base consonant
        // still matches on its own below; the nuqta mark itself falls
        // through the parser unrecognized and is silently skipped, which is
        // exactly the documented "falls back to the base letter" behavior.
        _ => return None,
    })
}

/// The homorganic nasal consonant for a stop `c` immediately following an
/// anusvara - one row of the standard Devanagari stop-consonant grid per
/// place of articulation - or `None` if `c` is not one of those 20 stops
/// (including that row's own nasal, since a following nasal is still
/// homorganic), meaning the anusvara nasalizes the vowel instead.
fn homorganic_nasal(c: char) -> Option<&'static str> {
    Some(match c {
        'क' | 'ख' | 'ग' | 'घ' | 'ङ' => "\u{14b}", // ŋ, velar
        'च' | 'छ' | 'ज' | 'झ' | 'ञ' => "\u{272}", // ɲ, palatal
        'ट' | 'ठ' | 'ड' | 'ढ' | 'ण' => "\u{273}", // ɳ, retroflex
        'त' | 'थ' | 'द' | 'ध' | 'न' => "n",       // dental
        'प' | 'फ' | 'ब' | 'भ' | 'म' => "m",       // labial
        _ => return None,
    })
}

/// What an anusvara/chandrabindu resolves to.
enum Nasal {
    None,
    /// Nasalize the vowel/schwa it follows: chandrabindu always does this;
    /// so does anusvara, except before a stop consonant.
    OnVowel,
    /// Anusvara before a stop consonant assimilates to that consonant's own
    /// place of articulation instead, as a distinct nasal segment - see
    /// [`homorganic_nasal`].
    Consonant(&'static str),
}

/// Anusvara/chandrabindu and visarga starting at `i`. Returns the index past
/// whatever was consumed, how an anusvara/chandrabindu (if any) resolves,
/// and whether a visarga was present.
fn consume_nasal_visarga(chars: &[char], mut i: usize) -> (usize, Nasal, bool) {
    let nasal = match chars.get(i) {
        Some('\u{901}') => {
            i += 1;
            Nasal::OnVowel
        }
        Some('\u{902}') => {
            i += 1;
            match chars.get(i).copied().and_then(homorganic_nasal) {
                Some(ph) => Nasal::Consonant(ph),
                None => Nasal::OnVowel,
            }
        }
        _ => Nasal::None,
    };
    let visarga = chars.get(i) == Some(&'\u{903}');
    if visarga {
        i += 1;
    }
    (i, nasal, visarga)
}

/// Push whatever `consume_nasal_visarga` decided: a vowel unit for `vowel`
/// (nasalized if that's what the anusvara/chandrabindu called for), then an
/// assimilated-anusvara consonant and/or a visarga /h/ if present.
fn push_vowel_with_modifiers(units: &mut Vec<Unit>, vowel: &str, nasal: Nasal, visarga: bool) {
    let mut s = vowel.to_string();
    if matches!(nasal, Nasal::OnVowel) {
        s.push('\u{303}');
    }
    units.push(Unit::Vowel(s));
    if let Nasal::Consonant(ph) = nasal {
        units.push(Unit::Consonant(ph.to_string()));
    }
    if visarga {
        units.push(Unit::Consonant("h".to_string()));
    }
}

/// Devanagari text -> a left-to-right [`Unit`] sequence, schwas still
/// unresolved.
fn parse_units(chars: &[char]) -> Vec<Unit> {
    let mut units = Vec::new();
    let mut i = 0;
    let n = chars.len();
    while i < n {
        let c = chars[i];

        if let Some(v) = independent_vowel(c) {
            i += 1;
            let (ni, nasal, visarga) = consume_nasal_visarga(chars, i);
            i = ni;
            push_vowel_with_modifiers(&mut units, v, nasal, visarga);
            continue;
        }

        if let Some(base) = consonant(c) {
            units.push(Unit::Consonant(base.to_string()));
            i += 1;
            match chars.get(i) {
                Some('\u{94d}') => {
                    i += 1; // virama: no vowel, consonant cluster continues
                }
                Some(&m) if matra(m).is_some() => {
                    let vph = matra(m).unwrap();
                    i += 1;
                    let (ni, nasal, visarga) = consume_nasal_visarga(chars, i);
                    i = ni;
                    push_vowel_with_modifiers(&mut units, vph, nasal, visarga);
                }
                _ => {
                    let (ni, nasal, visarga) = consume_nasal_visarga(chars, i);
                    i = ni;
                    units.push(Unit::Schwa {
                        nasal: matches!(nasal, Nasal::OnVowel),
                    });
                    if let Nasal::Consonant(ph) = nasal {
                        units.push(Unit::Consonant(ph.to_string()));
                    }
                    if visarga {
                        units.push(Unit::Consonant("h".to_string()));
                    }
                }
            }
            continue;
        }

        // Unrecognized character (e.g. a bare nuqta mark): skip.
        i += 1;
    }
    units
}

/// The unit at `i - offset`, or `None` if that would go before the start.
fn at(units: &[Unit], i: usize, offset: usize) -> Option<&Unit> {
    i.checked_sub(offset).and_then(|idx| units.get(idx))
}

/// Resolve every [`Unit::Schwa`] into either a kept vowel or a deleted one
/// (which simply leaves behind a bare [`Unit::Consonant`]), in place.
///
/// Word-final: always deleted (a word never surfaces with its last bare
/// consonant's inherent vowel). Word-medial: Ohala's rule, scanned
/// right-to-left so that an earlier (more leftward) decision sees the
/// already-resolved consonant cluster to its right - see the module docs
/// for the rule itself and citations.
fn resolve_schwas(units: &mut Vec<Unit>) {
    if let Some(Unit::Schwa { .. }) = units.last() {
        units.pop();
    }

    // Everything at or past whatever index the scan currently sits at has
    // already been resolved (right-to-left), so a `Schwa` unit still present
    // there is guaranteed to survive - it would already be gone if deleted.
    // The rule's own notation (patterns like "VCaCV") treats the schwa
    // itself as occupying a vowel slot for exactly this reason, so both
    // contexts below count `Schwa` the same as `Vowel`.
    let is_v = |u: Option<&Unit>| matches!(u, Some(Unit::Vowel(_)) | Some(Unit::Schwa { .. }));

    let mut i = units.len();
    while i > 0 {
        i -= 1;
        if !matches!(units[i], Unit::Schwa { .. }) {
            continue;
        }

        let right_ok =
            matches!(units.get(i + 1), Some(Unit::Consonant(_))) && is_v(units.get(i + 2));

        let one_c_before = matches!(at(units, i, 1), Some(Unit::Consonant(_)));
        let vc = one_c_before && is_v(at(units, i, 2));
        let two_c_before = one_c_before && matches!(at(units, i, 2), Some(Unit::Consonant(_)));
        let vcc = two_c_before && is_v(at(units, i, 3));
        let left_ok = vc || vcc;

        if left_ok && right_ok {
            units.remove(i);
        }
    }
}

fn emit(units: &[Unit]) -> String {
    let mut out = String::new();
    for u in units {
        match u {
            Unit::Vowel(s) | Unit::Consonant(s) => out.push_str(s),
            Unit::Schwa { nasal } => {
                out.push('\u{259}');
                if *nasal {
                    out.push('\u{303}');
                }
            }
        }
    }
    out
}

fn phonemize_word(word: &str) -> String {
    let chars: Vec<char> = word.chars().filter(|c| !c.is_whitespace()).collect();
    let mut units = parse_units(&chars);
    resolve_schwas(&mut units);
    emit(&units)
}

pub(crate) fn phonemize(text_in: &str) -> Result<String, String> {
    text::phonemize_clauses(text_in, |run| {
        let words: Vec<&str> = run.split_whitespace().collect();
        Ok(words
            .iter()
            .map(|w| phonemize_word(w))
            .collect::<Vec<_>>()
            .join(" "))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn word_final_inherent_vowel_is_deleted() {
        // राम "rāma" written, "rām" spoken: no schwa after the final म.
        let ps = phonemize_word("राम");
        assert!(!ps.ends_with('\u{259}'), "{ps}: word-final schwa should delete");
    }

    #[test]
    fn a_vowel_sign_overrides_the_inherent_vowel() {
        // दिन "din": the ि matra replaces न's default schwa with ɪ.
        let ps = phonemize_word("दिन");
        assert!(ps.contains('\u{26a}'), "{ps}: matra vowel missing");
    }

    #[test]
    fn virama_suppresses_the_vowel_for_a_cluster() {
        // विद्या has a व्+द cluster via virama: no vowel between द and य.
        let ps = phonemize_word("विद्या");
        // Should not have two schwas in a row from an un-suppressed cluster.
        assert!(!ps.contains("\u{259}\u{259}"));
    }

    #[test]
    fn medial_schwa_deletes_in_vc_c_v_context() {
        // नमकीन "namkeen" (salty): न(ə)-म(ə)-क-ी-न. The म's schwa sits
        // between "am" (VC) and "kī" (CV) - Ohala's rule deletes it, giving
        // "namkīn", not "namakīna". This is the module's whole reason to
        // exist: the word-final rule alone gets न right but leaves the
        // medial म schwa untouched.
        let ps = phonemize_word("नमकीन");
        assert!(
            !ps.contains(concat!("m", "\u{259}", "k")),
            "{ps}: medial schwa between m and k should have deleted"
        );
    }

    #[test]
    fn known_exception_kamal_falls_out_of_the_general_rule_naturally() {
        // कमल "kamal" (lotus): unlike most CVCVC words, both non-final
        // schwas survive - a commonly cited exception in the literature.
        // Verified this needs no special-casing: after word-final deletion
        // removes ल's schwa outright, म's schwa has a consonant (ल) but no
        // vowel after it (right context fails), so it survives; the same
        // then holds one step further left for क.
        let ps = phonemize_word("कमल");
        assert_eq!(ps, "k\u{259}m\u{259}l");
    }

    #[test]
    fn anusvara_before_a_stop_becomes_a_homorganic_consonant() {
        // जंगल "jangal" (forest): the anusvara sits before ग (a velar stop),
        // so it assimilates to /ŋ/ - a real consonant segment - rather than
        // nasalizing ज's vowel the way it would before a vowel or a
        // non-stop consonant.
        let ps = phonemize_word("जंगल");
        assert!(ps.contains('\u{14b}'), "{ps}: expected ŋ from the anusvara");
        assert!(!ps.contains('\u{303}'), "{ps}: should not also nasalize the vowel");
    }

    #[test]
    fn anusvara_before_a_vowel_or_non_stop_nasalizes_instead() {
        // हंस (hans, swan): anusvara before स (not a stop) nasalizes the
        // vowel instead of assimilating to a consonant.
        let ps = phonemize_word("हंस");
        assert!(ps.contains('\u{303}'), "{ps}: expected a nasalized vowel");
    }

    #[test]
    fn only_emits_known_vocabulary() {
        for word in [
            "नमस्ते", "आप", "आज", "कैसे", "हैं", "राम", "हिन्दी", "धन्यवाद", "नमकीन", "जंगल",
            "कमल", "हंस",
        ] {
            let ps = phonemize_word(word);
            for c in ps.chars() {
                assert!(crate::g2p::is_known(c), "{word} -> {ps} has unknown {c:?}");
            }
        }
    }
}
