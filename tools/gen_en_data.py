#!/usr/bin/env python3
"""Generate the English lexicons used by src/g2p/en.rs.

Ports, offline: nothing algorithmic - this just merges misaki's own
pronunciation dictionaries into the flat tables en.rs looks words up in.
misaki's entries are already spelled in Kokoro's phoneme alphabet (ties
resolved, stress marks placed), so no rewrite step is needed here the way
zh/ja's generators need one.

Many of misaki's entries are dictionaries keyed by part-of-speech -
homographs like "record"/"object"/"live" that are pronounced differently as
a verb than as their default (usually noun/adjective) reading, mostly
following English's productive noun-front-stress/verb-back-stress pattern
("RECord" vs "reCORD"). en.rs does not do part-of-speech tagging, so the main
table below collapses each entry to its "DEFAULT" pronunciation - but where a
distinct "VERB" pronunciation exists, it is *also* kept, in a second table,
for en.rs's verb-context heuristic (see the module doc in src/g2p/en.rs) to
use when the preceding word is an infinitive marker or modal. Everything
that isn't in that second table simply behaves as before: DEFAULT always,
homographs the heuristic can't resolve will sometimes come out wrong - an
accepted, documented gap.

Inputs (downloaded next to this script; not committed, regenerate on demand):
  us_gold.json, us_silver.json, gb_gold.json, gb_silver.json
    from hexgrad/misaki, misaki/data/*.json

Outputs:
  src/g2p/en_us.tsv        word \t default phonemes, one per line, sorted
  src/g2p/en_gb.tsv        same, British dictionary
  src/g2p/en_us_verb.tsv   word \t verb-tagged phonemes, only where distinct
  src/g2p/en_gb_verb.tsv   same, British dictionary
"""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(os.path.dirname(HERE), "src", "g2p")


def flatten(d):
    out = {}
    for k, v in d.items():
        if isinstance(v, str):
            out[k] = v
        elif isinstance(v, dict):
            out[k] = v.get("DEFAULT") or next(iter(v.values()))
    return out


def verb_alternates(d):
    """word -> VERB-tagged pronunciation, only for lowercase entries where it
    differs from DEFAULT (i.e. a genuine homograph, not just an entry that
    happens to carry a redundant VERB tag equal to its default reading)."""
    out = {}
    for k, v in d.items():
        if not (isinstance(v, dict) and k.islower()):
            continue
        default = v.get("DEFAULT")
        verb = v.get("VERB")
        if verb and default and verb != default:
            out[k] = verb
    return out


def main():
    for tag in ["us", "gb"]:
        gold_path = os.path.join(HERE, f"{tag}_gold.json")
        silver_path = os.path.join(HERE, f"{tag}_silver.json")
        if not (os.path.isfile(gold_path) and os.path.isfile(silver_path)):
            raise SystemExit(
                f"missing {gold_path} / {silver_path} - download misaki's "
                f"misaki/data/{tag}_gold.json and {tag}_silver.json next to "
                "this script first"
            )
        gold_raw = json.load(open(gold_path))
        silver_raw = json.load(open(silver_path))

        gold = flatten(gold_raw)
        silver = flatten(silver_raw)
        merged = {**silver, **gold}  # gold takes priority over silver
        out_path = os.path.join(OUT, f"en_{tag}.tsv")
        with open(out_path, "w") as f:
            for word in sorted(merged):
                phonemes = merged[word]
                assert "\t" not in word and "\n" not in word
                assert "\t" not in phonemes and "\n" not in phonemes
                f.write(f"{word}\t{phonemes}\n")
        print(f"{out_path}: {len(merged)} entries")

        verb_silver = verb_alternates(silver_raw)
        verb_gold = verb_alternates(gold_raw)
        verb_merged = {**verb_silver, **verb_gold}
        verb_out_path = os.path.join(OUT, f"en_{tag}_verb.tsv")
        with open(verb_out_path, "w") as f:
            for word in sorted(verb_merged):
                phonemes = verb_merged[word]
                assert "\t" not in word and "\n" not in word
                assert "\t" not in phonemes and "\n" not in phonemes
                f.write(f"{word}\t{phonemes}\n")
        print(f"{verb_out_path}: {len(verb_merged)} entries")


if __name__ == "__main__":
    main()
