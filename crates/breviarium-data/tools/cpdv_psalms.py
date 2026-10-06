#!/usr/bin/env python3
"""Check the complete English psalter against the retained CPDV source.

The public-domain source is the publisher's Psalms text:
https://www.sacredbible.org/catholic/OT-21_Psalms.htm
Fixtures retain its original verse text and explicitly record the superscriptions,
colophon, and Alleluia endings absent from the Latin breviary's psalm text.
Verse divisions follow Latin.
Run with Python 3 and PyYAML; no network access is needed.
"""

from __future__ import annotations

import argparse
from collections import Counter
import json
from pathlib import Path
import re
import sys

import yaml


CRATE = Path(__file__).resolve().parent.parent
ENTRY = re.compile(r"^psalm\.(\d+)-")
VERSE = re.compile(r"^(\d+):(\d+[a-z]?)\s+")
INLINE_REFERENCE = re.compile(r"\(\d+[a-z]?\)")
MARK = re.compile(r"[*†‡]")
ALLELUIA = re.compile(r"\ballel[uú][ií]a\b", re.IGNORECASE)
TOKEN = re.compile(r"\w+|[^\w\s]")
INVITATORY = "repeat full invitatory antiphon"
ENGLISH_INVITATORY = "Repeat the full invitatory antiphon."
BOW_RUBRICS = ("(bow)", "(a bow is made)", "(fit reverentia)")
ACROSTICS = (
    "ALEPH", "BETH", "GHIMEL", "DALETH", "HE", "VAU", "ZAIN", "HETH",
    "TETH", "IOD", "CAPH", "LAMED", "MEM", "NUN", "SAMECH", "AIN",
    "PHE", "SADE", "COPH", "RES", "SIN", "TAU",
)
LATIN_ACROSTICS = (
    "Aleph", "Beth", "Ghimel", "Daleth", "He", "Vav", "Zai", "Heth",
    "Teth", "Ioth", "Caph", "Lamed", "Mem", "Nun", "Samech", "Ain",
    "Phe", "Sade", "Coph", "Res", "Sin", "Thau",
)
ACROSTIC_LABEL = re.compile(
    r"^\((?:" + "|".join(LATIN_ACROSTICS) + r")\)\s*"
)


def source_bodies(source: dict, exclusions: dict) -> dict[int, str]:
    """Apply only documented, exact exclusions and canonical acrostic headings."""
    full = exclusions["full_verses"]
    prefixes = exclusions["prefixes"]
    suffixes = exclusions.get("suffixes", {})
    if set(full) & (set(prefixes) | set(suffixes)):
        raise ValueError("a fully excluded source verse also has a partial exclusion")
    unknown = (set(full) | set(prefixes) | set(suffixes)) - set(source)
    if unknown:
        raise ValueError(f"exclusions reference missing source verses: {sorted(unknown)}")
    chapters: dict[int, dict[int, str]] = {}
    for ref, text in source.items():
        if not re.fullmatch(r"\d+:\d+", ref) or not isinstance(text, str):
            raise ValueError(f"invalid CPDV source record: {ref!r}")
        chapter, verse = map(int, ref.split(":"))
        chapters.setdefault(chapter, {})[verse] = text
    if set(chapters) != set(range(1, 151)):
        raise ValueError("CPDV source must contain Psalms 1 through 150")

    bodies = {}
    for chapter, verses in sorted(chapters.items()):
        if set(verses) != set(range(1, max(verses) + 1)):
            raise ValueError(f"CPDV Psalm {chapter} source verse numbers have gaps")
        body = []
        for verse, text in sorted(verses.items()):
            ref = f"{chapter}:{verse}"
            if ref in full:
                if text != full[ref]["text"]:
                    raise ValueError(f"{ref}: full exclusion differs from original source")
                continue
            if ref in prefixes:
                prefix = prefixes[ref]["text"]
                if not prefix or not text.startswith(prefix):
                    raise ValueError(f"{ref}: exclusion is not an exact source prefix")
                text = text[len(prefix):]
            if ref in suffixes:
                suffix = suffixes[ref]["text"]
                if not suffix or not text.endswith(suffix):
                    raise ValueError(f"{ref}: exclusion is not an exact source suffix")
                text = text[:-len(suffix)]
            if chapter == 118 and (verse - 1) % 8 == 0:
                heading = ACROSTICS[(verse - 1) // 8] + ". "
                if not text.startswith(heading):
                    raise ValueError(f"{ref}: missing canonical acrostic heading {heading!r}")
                text = text[len(heading):]
            if not text.strip():
                raise ValueError(f"{ref}: source body is empty")
            body.append(text)
        bodies[chapter] = " ".join(body)
    return bodies


def prose(text: str, chapter: int, latin: str) -> str:
    """Remove known notation, preserving ordinary parentheses and punctuation."""
    text = VERSE.sub("", text, count=1)
    text = INLINE_REFERENCE.sub("", text)
    if chapter == 118:
        text = ACROSTIC_LABEL.sub("", text, count=1)
    if "(fit reverentia)" in latin:
        for rubric in BOW_RUBRICS:
            text = text.replace(rubric, "")
    return text


def difference(expected: list[str], actual: list[str]) -> str:
    index = next(
        (i for i, pair in enumerate(zip(expected, actual)) if pair[0] != pair[1]),
        min(len(expected), len(actual)),
    )
    start = max(0, index - 7)
    return (
        f"CPDV text differs at token {index + 1} "
        f"(source {len(expected)} tokens, English {len(actual)}):\n"
        f"    source:  {' '.join(expected[start:index + 10])!r}\n"
        f"    English: {' '.join(actual[start:index + 10])!r}"
    )


def check_entries(document: dict, bodies: dict[int, str]) -> tuple[int, list[str]]:
    errors = []
    counts: Counter[int] = Counter()
    for key, entry in document["texts"].items():
        match = ENTRY.match(key)
        if not match:
            continue
        chapter = int(match[1])
        counts[chapter] += 1
        label = f"{key}"
        if chapter not in bodies:
            errors.append(f"{label}: chapter is outside the CPDV psalter")
            continue
        content = entry.get("content", {})
        latin, english = content.get("la"), content.get("en")
        if not isinstance(latin, list) or not isinstance(english, list):
            errors.append(f"{label}: Latin and English must both be node lists")
            continue
        if len(latin) != len(english):
            errors.append(f"{label}: node count differs (Latin {len(latin)}, English {len(english)})")
        actual_body = []
        for index, (la_node, en_node) in enumerate(zip(latin, english), 1):
            where = f"{label}, node {index}"
            if not isinstance(la_node, dict) or not isinstance(en_node, dict):
                errors.append(f"{where}: nodes must be mappings")
                continue
            if {k: v for k, v in la_node.items() if k != "text"} != {
                k: v for k, v in en_node.items() if k != "text"
            }:
                errors.append(f"{where}: node type or metadata differs from Latin")
            la_text, en_text = la_node.get("text"), en_node.get("text")
            if not isinstance(la_text, str) or not isinstance(en_text, str):
                errors.append(f"{where}: node text must be a string")
                continue
            la_ref, en_ref = VERSE.match(la_text), VERSE.match(en_text)
            if not la_ref:
                if chapter != 94 or la_text != INVITATORY or en_text not in (
                    INVITATORY, ENGLISH_INVITATORY
                ):
                    errors.append(f"{where}: unsupported or changed unnumbered node")
                continue
            if not en_ref or la_ref.groups() != en_ref.groups():
                errors.append(f"{where}: English verse prefix differs from Latin")
            if int(la_ref[1]) != chapter:
                errors.append(f"{where}: Latin verse prefix has the wrong chapter")
            if MARK.findall(en_text) != MARK.findall(la_text):
                errors.append(f"{where}: * † ‡ sequence differs from Latin")
            if INLINE_REFERENCE.findall(en_text) != INLINE_REFERENCE.findall(la_text):
                errors.append(f"{where}: embedded verse references differ from Latin")
            if len(ALLELUIA.findall(en_text)) != len(ALLELUIA.findall(la_text)):
                errors.append(f"{where}: Alleluia count differs from Latin")
            if chapter == 118:
                la_heading = ACROSTIC_LABEL.match(VERSE.sub("", la_text, count=1))
                en_heading = ACROSTIC_LABEL.match(VERSE.sub("", en_text, count=1))
                if (la_heading.group().strip() if la_heading else None) != (
                    en_heading.group().strip() if en_heading else None
                ):
                    errors.append(f"{where}: acrostic label differs from Latin")
            body = prose(en_text, chapter, la_text)
            if any(not re.search(r"\w", half) for half in MARK.split(body)):
                errors.append(f"{where}: chant division has an empty half verse")
            actual_body.append(MARK.sub(" ", body))
        expected = TOKEN.findall(bodies[chapter])
        actual = TOKEN.findall(" ".join(actual_body))
        if actual != expected:
            errors.append(f"{label}: {difference(expected, actual)}")
    expected_counts = Counter({chapter: 2 if chapter == 94 else 1 for chapter in range(1, 151)})
    if counts != expected_counts:
        for chapter in sorted(set(counts) | set(expected_counts)):
            if counts[chapter] != expected_counts[chapter]:
                errors.append(
                    f"Psalm {chapter}: expected {expected_counts[chapter]} entries, found {counts[chapter]}"
                )
    return sum(counts.values()), errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--psalms", type=Path, default=CRATE / "data" / "lexicon" / "psalms.yaml",
        help="psalms YAML to check (default: the active lexicon)",
    )
    args = parser.parse_args()
    try:
        source = json.loads((CRATE / "cpdv" / "psalms.json").read_text(encoding="utf-8"))
        exclusions = json.loads((CRATE / "cpdv" / "source-exclusions.json").read_text(encoding="utf-8"))
        bodies = source_bodies(source, exclusions)
        document = yaml.safe_load(args.psalms.read_text(encoding="utf-8"))
        count, errors = check_entries(document, bodies)
    except (OSError, ValueError, KeyError, TypeError, yaml.YAMLError) as error:
        print(f"CPDV psalter check failed: {error}", file=sys.stderr)
        return 1
    if errors:
        print("\n".join(errors), file=sys.stderr)
        print(f"CPDV psalter check failed: {len(errors)} errors.", file=sys.stderr)
        return 1
    print(f"Verified {count} English psalm entries against CPDV (all 150 psalms and both Psalm 94 variants).")
    print("Ordered words, punctuation, verse prefixes, node structure, and chant divisions match.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
