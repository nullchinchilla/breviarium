#!/usr/bin/env python3
"""Merge per-chunk zh translation files into one {latin: zh} dict and validate.

Reads every ``zh/full/chunk-*.json`` ({latin: chinese} objects, optional
``_notes`` key), merges them, and checks coverage against ``zh/latin.json``:
which strings are still untranslated and which chunk files are missing. Writes
the merged dict to ``zh/chinese.json`` (the input to ``en2.py --lang zh apply``).
"""
import json
import glob
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent
CRATE = HERE.parent
ZH = CRATE / "zh"
CHUNK_SIZE = 130  # keep in sync with the workflow's default


def main():
    latin = json.load(open(ZH / "latin.json", encoding="utf-8"))
    latin_set = set(latin)
    total = len(latin)
    n_chunks = (total + CHUNK_SIZE - 1) // CHUNK_SIZE

    merged = {}
    notes = {}
    present = set()
    bad_keys = 0
    for path in sorted(glob.glob(str(ZH / "full" / "chunk-*.json"))):
        idx = int(re.search(r"chunk-(\d+)", path).group(1))
        present.add(idx)
        try:
            d = json.load(open(path, encoding="utf-8"))
        except Exception as e:
            print(f"  !! {Path(path).name}: unreadable ({e})")
            continue
        if "_notes" in d:
            notes[idx] = d.pop("_notes")
        for k, v in d.items():
            if k not in latin_set:
                bad_keys += 1
                continue  # key not a known Latin string (altered/whitespace)
            merged[k] = v

    missing_chunks = sorted(set(range(n_chunks)) - present)
    untranslated = [s for s in latin if s not in merged]

    (ZH / "chinese.json").write_text(
        json.dumps(merged, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (ZH / "full" / "notes.json").write_text(
        json.dumps(notes, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    print(f"strings total      : {total}")
    print(f"chunk files present: {len(present)}/{n_chunks}")
    print(f"translated (unique): {len(merged)}")
    print(f"untranslated       : {len(untranslated)}")
    print(f"bad/unknown keys   : {bad_keys}")
    if missing_chunks:
        # collapse to ranges for easy re-run
        print(f"MISSING chunks ({len(missing_chunks)}): {missing_chunks}")
    print(f"wrote {ZH/'chinese.json'} and full/notes.json")
    # write untranslated indices so missing chunks can be targeted for re-run
    if untranslated:
        idxs = sorted({latin.index(s) // CHUNK_SIZE for s in untranslated})
        print(f"chunks needing (re)run: {idxs}")


if __name__ == "__main__":
    main()
