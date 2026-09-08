#!/usr/bin/env python3
"""Generate the Mandarin G2P tables used by src/g2p/zh_data.rs and zh_phrases.txt.

Ports, offline and without any Python dependencies:
  * pypinyin's strict initial/final analysis (pypinyin/standard.py, style/_utils.py)
  * misaki's pinyin -> IPA transcription tables (misaki/transcription.py)

Inputs (downloaded next to this script, see README section "Regenerating"):
  pinyin_dict.json, phrases_dict.json   from mozillazg/python-pinyin

Outputs:
  src/g2p/zh_syllables.rs   toneless syllable -> IPA, with '0' marking the tone slot
  src/g2p/zh_phrases.txt    word \t syllable syllable ...  (tone digits, neutral = 5)
"""
import json, os, sys, unicodedata

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(os.path.dirname(HERE), "src", "g2p")

# --- diacritic -> (plain letter, tone) -------------------------------------
TONE_MARKS = {
    "ā":("a",1),"á":("a",2),"ǎ":("a",3),"à":("a",4),
    "ē":("e",1),"é":("e",2),"ě":("e",3),"è":("e",4),
    "ī":("i",1),"í":("i",2),"ǐ":("i",3),"ì":("i",4),
    "ō":("o",1),"ó":("o",2),"ǒ":("o",3),"ò":("o",4),
    "ū":("u",1),"ú":("u",2),"ǔ":("u",3),"ù":("u",4),
    "ǖ":("ü",1),"ǘ":("ü",2),"ǚ":("ü",3),"ǜ":("ü",4),
    "ń":("n",2),"ň":("n",3),"ǹ":("n",4),
    "ḿ":("m",2), "m̄":("m",1), "m̀":("m",4),
    "ê̄":("ê",1),"ế":("ê",2),"ê̌":("ê",3),"ề":("ê",4),
}

def split_tone(py):
    """Accented pinyin -> (toneless syllable using 'ü', tone 1-5)."""
    py = unicodedata.normalize("NFC", py)
    tone = 5
    out = []
    i = 0
    while i < len(py):
        # try two-char sequences first (ê with combining marks)
        two = py[i:i+2]
        if two in TONE_MARKS:
            p, t = TONE_MARKS[two]; out.append(p); tone = t; i += 2; continue
        ch = py[i]
        if ch in TONE_MARKS:
            p, t = TONE_MARKS[ch]; out.append(p); tone = t
        elif ch == "v":
            out.append("ü")
        else:
            out.append(ch)
        i += 1
    return "".join(out), tone

# --- pypinyin strict analysis ----------------------------------------------
INITIALS = ["zh","ch","sh","b","p","m","f","d","t","n","l","g","k","h",
            "j","q","x","r","z","c","s"]  # longest-first for prefix matching
FINALS = {"i","u","ü","a","ia","ua","o","uo","e","ie","üe","ai","uai","ei","uei",
          "ao","iao","ou","iou","an","ian","uan","üan","en","in","uen","ün",
          "ang","iang","uang","eng","ing","ueng","ong","iong","er","ê"}

def convert_zero_consonant(py):
    raw = py
    if py.startswith("y"):
        rest = py[1:]
        first = rest[0] if rest else None
        if first == "u":       py = "ü" + py[2:]
        elif first == "i":     py = rest
        else:                  py = "i" + rest
    if raw.startswith("w"):
        rest = py[1:]
        first = rest[0] if rest else None
        py = rest if first == "u" else "u" + rest
    return py if py in FINALS else raw

def convert_finals(py):
    py = convert_zero_consonant(py)
    if len(py) > 1 and py[0] in "jqx" and py[1] == "u":     # ju -> jü
        py = py[0] + "ü" + py[2:]
    if py.endswith("iu"):   py = py[:-2] + "iou"
    if py.endswith("ui"):   py = py[:-2] + "uei"
    if py.endswith("un"):   py = py[:-2] + "uen"
    return py

def get_initial(py, strict=True):
    pool = INITIALS if strict else INITIALS + ["y","w"]
    for i in pool:
        if py.startswith(i):
            return i
    return ""

def get_final(py):
    conv = convert_finals(py)
    ini = get_initial(conv)
    fin = conv[len(ini):]
    if fin not in FINALS:                       # e.g. "yo"
        ini = get_initial(conv, strict=False)
        fin = conv[len(ini):]
        return fin if fin in FINALS else ""
    return fin

# --- misaki transcription tables -------------------------------------------
INITIAL_IPA = {
    "b":"p", "c":"ʦʰ", "ch":"ꭧʰ", "d":"t", "f":"f", "g":"k", "h":"x",
    "j":"ʨ", "k":"kʰ", "l":"l", "m":"m", "n":"n", "p":"pʰ", "q":"ʨʰ",
    "r":"ɻ", "s":"s", "sh":"ʂ", "t":"tʰ", "x":"ɕ", "z":"ʦ", "zh":"ꭧ",
}
FINAL_IPA = {
    "a":"a0", "ai":"ai̯0", "an":"a0n", "ang":"a0ŋ", "ao":"au̯0",
    "e":"ɤ0", "ei":"ei̯0", "en":"ə0n", "eng":"ə0ŋ",
    "i":"i0", "ia":"ja0", "ian":"jɛ0n", "iang":"ja0ŋ", "iao":"jau̯0",
    "ie":"je0", "in":"i0n", "iou":"jou̯0", "ing":"i0ŋ", "iong":"jʊ0ŋ",
    "ong":"ʊ0ŋ", "ou":"ou̯0", "u":"u0", "uei":"wei̯0", "ua":"wa0",
    "uai":"wai̯0", "uan":"wa0n", "uen":"wə0n", "uang":"wa0ŋ", "ueng":"wə0ŋ",
    "uo":"wo0", "o":"wo0", "ü":"y0", "üe":"ɥe0", "üan":"ɥɛ0n", "ün":"y0n",
}
FINAL_IPA_AFTER_ZH_CH_SH_R = {"i":"ɻ̩0"}
FINAL_IPA_AFTER_Z_C_S      = {"i":"ɹ̩0"}
SYLLABIC_CONSONANT_IPA = {"hm":"hm0", "hng":"hŋ0", "m":"m0", "n":"n0", "ng":"ŋ0"}
INTERJECTION_IPA       = {"io":"jɔ0", "ê":"ɛ0", "er":"ɚ0", "o":"ɔ0"}

# misaki's ZHG2P.retone() folds the apical vowels onto a single symbol, and
# legacy_call() drops the non-syllabic breve; both are position independent, so
# we bake them in here and the runtime only has to substitute the tone letter.
def retone(ipa):
    return (ipa.replace("\u027b\u0329", "\u0268")   # ɻ̩ -> ɨ
               .replace("\u0279\u0329", "\u0268")   # ɹ̩ -> ɨ
               .replace("\u032f", ""))                # drop  ̯

def syllable_to_ipa(syl):
    """Toneless pinyin syllable -> IPA with '0' where the tone letter goes."""
    if syl in INTERJECTION_IPA:        return INTERJECTION_IPA[syl]
    if syl in SYLLABIC_CONSONANT_IPA:  return SYLLABIC_CONSONANT_IPA[syl]
    ini = get_initial(convert_finals(syl))
    fin = get_final(syl)
    if not fin:
        return None
    if ini and ini not in INITIAL_IPA:
        return None
    if ini in ("zh","ch","sh","r") and fin in FINAL_IPA_AFTER_ZH_CH_SH_R:
        fin_ipa = FINAL_IPA_AFTER_ZH_CH_SH_R[fin]
    elif ini in ("z","c","s") and fin in FINAL_IPA_AFTER_Z_C_S:
        fin_ipa = FINAL_IPA_AFTER_Z_C_S[fin]
    elif fin in FINAL_IPA:
        fin_ipa = FINAL_IPA[fin]
    else:
        return None
    return retone((INITIAL_IPA[ini] if ini else "") + fin_ipa)

# --- collect every syllable pypinyin can emit ------------------------------
def load(name):
    p = os.path.join(HERE, name)
    if not os.path.exists(p):
        sys.exit(f"missing {p} - see the 'Regenerating' section of the README")
    with open(p, encoding="utf8") as f:
        return json.load(f)

char_dict   = load("pinyin_dict.json")
phrase_dict = load("phrases_dict.json")

syllables = set()
for readings in char_dict.values():
    for py in readings.split(","):
        syllables.add(split_tone(py.strip())[0])
phrase_lines = []
for word, readings in phrase_dict.items():
    sylls = []
    for cand in readings:
        s, t = split_tone(cand[0])
        syllables.add(s)
        sylls.append(f"{s}{t}")
    phrase_lines.append(f"{word}\t{' '.join(sylls)}")

# Every phoneme we emit must be a token the Kokoro model actually knows;
# anything else would be dropped at tokenisation and silently mispronounced.
KOKORO_PHONEMES = set(
    "0;:,.!?\u2014\u2026\"()\u201c\u201d \u0303\u02a3\u02a5\u02a6\u02a8\u1d5d\uab67"
    "AIOQSTWY\u1d4aabcdefhijklmnopqrstuvwxyz"
    "\u0251\u0250\u0252\u00e6\u03b2\u0254\u0255\u00e7\u0256\u00f0\u02a4\u0259"
    "\u025a\u025b\u025c\u025f\u0261\u0265\u0268\u026a\u029d\u026f\u0270\u014b"
    "\u0273\u0272\u0274\u00f8\u0278\u03b8\u0153\u0279\u027e\u027b\u0281\u027d"
    "\u0282\u0283\u0288\u02a7\u028a\u028b\u028c\u0263\u0264\u03c7\u028e\u0292"
    "\u0294\u02c8\u02cc\u02d0\u02b0\u02b2\u2193\u2192\u2197\u2198\u1d7b"
)

table, skipped = [], []
for s in sorted(syllables):
    ipa = syllable_to_ipa(s)
    if not ipa:
        skipped.append(s)
        continue
    bad = sorted({c for c in ipa if c not in KOKORO_PHONEMES})
    if bad:
        sys.exit(f"syllable {s!r} -> {ipa!r} uses non-Kokoro phonemes {bad}")
    table.append((s, ipa))

os.makedirs(OUT, exist_ok=True)
with open(os.path.join(OUT, "zh_syllables.rs"), "w", encoding="utf8") as f:
    f.write("// @generated by tools/gen_zh_data.py - do not edit by hand.\n")
    f.write("//\n// Toneless pinyin syllable -> IPA, with '0' marking the slot the tone\n")
    f.write("// letter is substituted into. Ported from misaki/transcription.py.\n\n")
    f.write("pub(crate) static SYLLABLES: &[(&str, &str)] = &[\n")
    for s, ipa in table:
        f.write(f'    ("{s}", "{ipa}"),\n')
    f.write("];\n")

with open(os.path.join(OUT, "zh_phrases.txt"), "w", encoding="utf8") as f:
    f.write("\n".join(sorted(phrase_lines)))
    f.write("\n")

print(f"syllables: {len(table)} written, {len(skipped)} skipped")
if skipped:
    print("  skipped:", " ".join(skipped[:40]))
print(f"phrases:   {len(phrase_lines)}")
