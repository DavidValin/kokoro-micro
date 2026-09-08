# Regenerating the language tables

`src/g2p/zh_syllables.rs`, `src/g2p/zh_phrases.txt` and `src/g2p/ja_data.rs`
are generated. They are committed, so a normal build never runs anything here;
you only need this when updating against a newer upstream.

Fetch the inputs, then run the generators from this directory:

```sh
curl -LO https://raw.githubusercontent.com/mozillazg/python-pinyin/master/pypinyin/pinyin_dict.json
curl -LO https://raw.githubusercontent.com/mozillazg/python-pinyin/master/pypinyin/phrases_dict.json
curl -LO https://raw.githubusercontent.com/hexgrad/misaki/main/misaki/cutlet.py

python3 gen_zh_data.py    # Mandarin: syllable -> IPA, and the phrase readings
python3 gen_ja_data.py    # Japanese: kana -> phonemes
```

Both generators validate every phoneme they emit against `kokoro_vocab.json`
(a copy of the `vocab` object from Kokoro-82M's `config.json`) and refuse to
write a table containing a symbol the model has no token for. That check is the
point of having them: a phoneme outside the vocabulary is dropped silently at
tokenisation, so it would show up as a missing sound rather than an error.

## Sources and licences

| input | from | licence |
|---|---|---|
| `pinyin_dict.json`, `phrases_dict.json` | [mozillazg/python-pinyin](https://github.com/mozillazg/python-pinyin) | MIT |
| `cutlet.py` | [hexgrad/misaki](https://github.com/hexgrad/misaki), adapted from [polm/cutlet](https://github.com/polm/cutlet) | Apache-2.0 / MIT |
| transcription tables in `gen_zh_data.py` | [hexgrad/misaki](https://github.com/hexgrad/misaki), adapted from [stefantaubert/pinyin-to-ipa](https://github.com/stefantaubert/pinyin-to-ipa) | Apache-2.0 / MIT |
