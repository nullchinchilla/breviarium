#!/usr/bin/env python3
"""Merge per-chunk zh translation files into one {latin: zh} dict and validate.

Reads every ``zh/full/chunk-*.json`` ({latin: chinese} objects, optional
``_notes`` key), merges them, and checks coverage against ``zh/latin.json``:
which strings are still untranslated and which chunk files are missing. Writes
the merged dict to ``zh/chinese.json`` (the input to ``en2.py --lang zh apply``).
"""
import argparse
import json
import glob
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent
CRATE = HERE.parent
CHUNK_SIZE = 130  # keep in sync with the workflow's default


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--lang", default="zhs", help="language dir under the crate (default: zhs)")
    a = ap.parse_args()
    ZH = CRATE / a.lang
    latin = json.load(open(ZH / "latin.json", encoding="utf-8"))
    latin_set = set(latin)
    total = len(latin)
    n_chunks = (total + CHUNK_SIZE - 1) // CHUNK_SIZE

    merged = {}
    notes = {}
    bad_keys = 0
    # Read every json in full/ (chunk-*.json from index runs AND seg-*.json from
    # range re-runs), keyed by the actual Latin string — so filename scheme and
    # overlap don't matter.
    for path in sorted(glob.glob(str(ZH / "full" / "*.json"))):
        name = Path(path).name
        if name == "notes.json":
            continue
        try:
            d = json.load(open(path, encoding="utf-8"))
        except Exception as e:
            print(f"  !! {name}: unreadable ({e})")
            continue
        if "_notes" in d:
            notes[name] = d.pop("_notes")
        for k, v in d.items():
            if k not in latin_set:
                bad_keys += 1
                continue  # key not a known Latin string (altered/whitespace)
            merged[k] = v

    untranslated = [s for s in latin if s not in merged]
    idx = {s: i for i, s in enumerate(latin)}
    missing_chunks = sorted({idx[s] // CHUNK_SIZE for s in untranslated})

    (ZH / "chinese.json").write_text(
        json.dumps(merged, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (ZH / "full" / "notes.json").write_text(
        json.dumps(notes, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    print(f"strings total      : {total}")
    print(f"json files read    : {len(glob.glob(str(ZH / 'full' / '*.json'))) - (1 if (ZH / 'full' / 'notes.json').exists() else 0)}")
    print(f"translated (unique): {len(merged)}")
    print(f"untranslated       : {len(untranslated)}")
    print(f"bad/unknown keys   : {bad_keys}")
    print(f"wrote {ZH/'chinese.json'} and full/notes.json")
    if missing_chunks:
        print(f"chunks needing (re)run ({len(missing_chunks)}): {missing_chunks}")


if __name__ == "__main__":
    main()
