#!/usr/bin/env python3
"""Shared access to the retained original, public-domain CUVS source.

The fixture uses standard USFM book codes and canonical verse keys such as
``PSA 1:1`` and ``LUK 1:46``. The source is the older BibleGateway CUVS text,
not the later Union Version with new punctuation. Divine names are deliberately
not changed by default: each liturgical quotation must follow its own Latin.
"""

from __future__ import annotations

from functools import lru_cache
import json
from pathlib import Path
import re
import unicodedata

CRATE = Path(__file__).resolve().parent.parent
SOURCE_PATH = CRATE / "cuv" / "source.json"
LEXICON = CRATE / "data" / "lexicon"
REFERENCE = re.compile(r"^([A-Z1-3]{3}) ([1-9]\d*):([1-9]\d*)$")
EDITORIAL_NOTE = re.compile(
    r"[（(](?:或(?:作|译)|原文|有(?:古)?卷|小字|下同)[^（）()]*[）)]"
)


@lru_cache(maxsize=4)
def load_source(path: str | Path = SOURCE_PATH) -> dict:
    """Return the full fixture document, including metadata and ``verses``."""
    document = json.loads(Path(path).read_text(encoding="utf-8"))
    verses = document.get("verses")
    if not isinstance(verses, dict) or not verses:
        raise ValueError("CUVS fixture must have a nonempty verses mapping")
    placeholders = set(document.get("metadata", {}).get("combined_verse_placeholders", []))
    for reference, text in verses.items():
        if not REFERENCE.fullmatch(reference) or not isinstance(text, str) or (not text and reference not in placeholders):
            raise ValueError(f"invalid CUVS source verse: {reference!r}")
    return document


def source_verses(path: str | Path = SOURCE_PATH) -> dict[str, str]:
    """Return the source mapping keyed by canonical ``BOOK chapter:verse``."""
    return load_source(path)["verses"]


def strip_editorial_notes(text: str) -> str:
    """Remove inline edition variants, retaining narrative parentheses.

    Only recognizable editorial labels are removed. Callers must explicitly
    record these omissions in their passage mappings; the fixture stays intact.
    """
    return EDITORIAL_NOTE.sub("", text)


def verse(
    book: str, chapter: int, number: int, *,
    divine_name: str | None = None, editorial_notes: bool = True,
) -> str:
    """Read one canonical verse; optionally replace 耶和华 and omit notes.

    ``editorial_notes=True`` preserves source notes, matching the raw fixture.
    A divine-name replacement must be explicitly chosen as 主 or 神.
    """
    text = source_verses()[f"{book} {chapter}:{number}"]
    if not editorial_notes:
        text = strip_editorial_notes(text)
    if divine_name is not None:
        if divine_name not in ("主", "神"):
            raise ValueError("CUV divine-name replacement must be 主 or 神")
        text = text.replace("耶和华", divine_name)
    return text


def passage(
    book: str, chapter: int, start: int, end: int | None = None, **options,
) -> str:
    """Join an inclusive verse range within one canonical chapter."""
    end = start if end is None else end
    if end < start:
        raise ValueError("passage end must not precede its start")
    return "".join(verse(book, chapter, number, **options) for number in range(start, end + 1))


def chapter(book: str, number: int) -> list[tuple[str, str]]:
    """Return ordered canonical reference/text pairs for one chapter."""
    prefix = f"{book} {number}:"
    rows = [(ref, text) for ref, text in source_verses().items() if ref.startswith(prefix)]
    if not rows:
        raise KeyError(f"{book} {number}")
    return sorted(rows, key=lambda row: int(row[0].split(":")[1]))


def normalized_text(text: str) -> str:
    """Retain ordered wording while ignoring punctuation and whitespace.

    This does not delete verse references, rubrics, or editorial notes. Strip
    explicitly registered liturgical notation before calling this comparison.
    """
    return "".join(
        character for character in text
        if not character.isspace()
        and unicodedata.category(character)[0] not in ("P", "Z")
        # Legacy CUV uses a box-drawing line as Chinese dash punctuation.
        and character not in "*†‡─"
    )


def load_lexicon(name: str) -> dict:
    """Load one active multilingual lexicon without rewriting its YAML."""
    import yaml

    path = LEXICON / (name if name.endswith(".yaml") else f"{name}.yaml")
    loader = getattr(yaml, "CSafeLoader", yaml.SafeLoader)
    return yaml.load(path.read_text(encoding="utf-8"), Loader=loader)["texts"]


if __name__ == "__main__":
    document = load_source()
    print(f"CUVS fixture: {len(document['verses'])} verses from {document['metadata']['source_url']}")
