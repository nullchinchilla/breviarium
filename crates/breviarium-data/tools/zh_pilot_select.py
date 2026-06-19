#!/usr/bin/env python3
"""Select a representative pilot slice of lexicon entries for zh translation.

Picks a curated set of complete entries spanning every role, plus the shortest
entry of each remaining role, then gathers their `la` prose strings
(deduplicated) as the agent input. Because we take *whole entries*, the partial
`apply` (zh) can emit a full column for each, so the pilot renders end-to-end.
"""
import json
import glob
from pathlib import Path

import yaml

HERE = Path(__file__).resolve().parent
CRATE = HERE.parent
LEX = CRATE / "data" / "lexicon"
OUT = CRATE / "zh" / "pilot"
TRANSLATABLE = ("text", "antiphon")

# Curated entries that exercise the hard cases (scripture → CUV/Studium fetch,
# canticle verse-prefixes, doctrinal prose, Marian antiphon).
CURATED = [
    "psalm.canticum-b-mari-virginis-luc-1-46-55-ce5eba84a7",  # Magnificat (Lk, CUV)
    "psalm.116-1-laud-te-d-minum-omnes-gentes-laud-te-eum-o-4793be2c32",  # Ps 116/117
    "psalm.133-1a-ecce-quam-bonum-et-quam-iuc-ndum-habit-re-4c4f40f041",  # Ps 133/134
]
# Roles to auto-fill with the shortest available entry (one each).
SHORTEST_ROLES = [
    "antiphon", "marian_antiphon", "responsory", "short_responsory",
    "chapter", "versicle", "invitatory", "short_reading", "hymn", "collect",
    "blessing", "preces",
]
# One extra short "reading" (patristic/scriptural lesson) — these run long, so
# take a short one to keep the pilot bounded.
READING_MAX_CHARS = 600


# Per-role floor on total `la` prose chars, so we pick a *substantive* entry
# rather than a structural stub ("#Oratio", "Deo grátias", a `…` placeholder).
ROLE_FLOOR = 60


def la_strings(entry):
    out = []
    for node in (entry.get("content") or {}).get("la") or []:
        if isinstance(node, dict):
            for key in TRANSLATABLE:
                v = node.get(key)
                if isinstance(v, str) and v.strip():
                    out.append(v)
    return out


def is_stub(strs):
    """Pointer/placeholder entries we don't want as pilot exemplars."""
    if not strs:
        return True
    joined = " ".join(strs).strip()
    if joined.startswith("#") or joined.endswith("…") or "…" in joined:
        return True
    return False


def main():
    all_entries = {}  # id -> (file, entry)
    for f in sorted(glob.glob(str(LEX / "*.yaml"))):
        doc = yaml.safe_load(open(f, encoding="utf-8")) or {}
        for k, e in (doc.get("texts") or {}).items():
            all_entries[k] = (Path(f).name, e or {})

    picked = {}  # id -> reason

    for cid in CURATED:
        if cid in all_entries:
            picked[cid] = "curated"

    # shortest *substantive* entry per role (above the floor, not a stub)
    by_role = {}
    for k, (fn, e) in all_entries.items():
        r = e.get("role")
        s = la_strings(e)
        if not s or is_stub(s):
            continue
        by_role.setdefault(r, []).append((sum(len(x) for x in s), k))
    for role in SHORTEST_ROLES:
        cands = [c for c in sorted(by_role.get(role, [])) if c[0] >= ROLE_FLOOR]
        if cands:
            picked.setdefault(cands[0][1], f"shortest:{role}")

    # one short reading
    for total, k in sorted(by_role.get("reading", [])):
        if total >= 120 and total <= READING_MAX_CHARS:
            picked.setdefault(k, "short:reading")
            break

    # gather strings (dedup, first-seen order) + per-entry record
    seen = {}
    records = []
    for k, reason in picked.items():
        fn, e = all_entries[k]
        strs = la_strings(e)
        for s in strs:
            seen.setdefault(s, None)
        records.append({"id": k, "file": fn, "role": e.get("role"),
                         "reason": reason, "strings": strs})

    OUT.mkdir(parents=True, exist_ok=True)
    unique = list(seen.keys())
    (OUT / "pilot-latin.json").write_text(
        json.dumps(unique, ensure_ascii=False, indent=0) + "\n", encoding="utf-8")
    (OUT / "entries.json").write_text(
        json.dumps(records, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    print(f"entries picked : {len(records)}")
    print(f"unique strings : {len(unique)}")
    print(f"total chars    : {sum(len(s) for s in unique)}")
    print("roles:", sorted({r['role'] for r in records}))
    for r in records:
        prev = r["strings"][0][:60].replace("\n", " ")
        print(f"  [{r['role']:<15}] {r['id'][:48]:<48} ({len(r['strings'])} str) {prev}")


if __name__ == "__main__":
    main()
