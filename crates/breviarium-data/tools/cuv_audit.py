#!/usr/bin/env python3
"""Check the ecumenical Chinese corpus against retained original CUV sources.

Run with Python 3 and PyYAML. No network access is needed. Source references
and individually retained edits reconstruct the reviewed wording; punctuation
may be modernized, while words and their order must remain identical.
"""
from __future__ import annotations

import argparse
from copy import deepcopy
import hashlib
import json
from pathlib import Path
import re
import sys

from cuv_common import CRATE, load_lexicon, load_source, normalized_text

AUDIT = CRATE / "cuv" / "audit.json"
PREFIX = re.compile(r"(?m)^\d+(?::\d+[a-z]?|[a-z]?)\s+")
MARKS = re.compile(r"[*†‡+]")


def digest(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def prose(value: str, record: dict) -> str:
    if record.get("strip_labels"):
        value = PREFIX.sub("", value)
    for annotation in sorted(record.get("notation", []), key=len, reverse=True):
        value = value.replace(annotation, "")
    return normalized_text(value).replace("+", "")


def expected_wording(record: dict, source: dict[str, str]) -> str:
    if record["source_kind"] == "latin":
        return record["expected_wording"]
    original = normalized_text("".join(source[ref] for ref in record["references"]))
    if digest(original) != record["source_sha256"]:
        raise ValueError("retained CUV source differs from reviewed source")
    previous = len(original) + 1
    result = original
    for edit in reversed(record["edits"]):
        start, end = edit["start"], edit["end"]
        if not 0 <= start <= end <= len(original) or end > previous:
            raise ValueError("overlapping or invalid source edit")
        if original[start:end] != edit["before"] or not edit.get("reason"):
            raise ValueError("source edit lacks matching original wording or a reason")
        result = result[:start] + edit["after"] + result[end:]
        previous = start
    return result


def check_record(record: dict, lexicons: dict, source: dict) -> list[str]:
    label = record["id"]
    errors = []
    values = []
    entry = lexicons[record["fields"][0]["file"]][label]
    latin = entry["content"].get("la", [])
    if digest(json.dumps(latin, ensure_ascii=False, sort_keys=True)) != record["latin_sha256"]:
        errors.append(f"{label}: Latin source differs from reviewed source")
    for field in record["fields"]:
        entry = lexicons[field["file"]][label]
        nodes = entry["content"]["zhs-ecu"]
        if len(nodes) != field["node_count"]:
            errors.append(f"{label}: node count changed")
            continue
        node = nodes[field["node_index"]]
        value = node[field["field"]]
        if node["type"] != field["type"]:
            errors.append(f"{label}: node type changed")
        if MARKS.findall(value) != field["marks"]:
            errors.append(f"{label}: chant marks changed at node {field['node_index']}")
        if PREFIX.findall(value) != field["prefixes"]:
            errors.append(f"{label}: printed verse prefixes changed at node {field['node_index']}")
        if "耶和华" in value:
            errors.append(f"{label}: unreplaced divine name")
        if record.get("chant_passage"):
            body = PREFIX.sub("", value)
            for annotation in record.get("notation", []):
                body = body.replace(annotation, "")
            halves = re.split(r"[*†‡]", body)
            if any(not normalized_text(part) for part in halves):
                errors.append(f"{label}: empty chant segment at node {field['node_index']}")
        values.append(value)
    try:
        expected = expected_wording(record, source)
    except (KeyError, ValueError) as error:
        return errors + [f"{label}: {error}"]
    actual = prose("\n".join(values), record)
    if actual != expected:
        offset = next((i for i, (a, b) in enumerate(zip(actual, expected)) if a != b), min(len(actual), len(expected)))
        errors.append(f"{label}: wording differs at character {offset + 1}: expected {expected[max(0, offset-8):offset+15]!r}, got {actual[max(0, offset-8):offset+15]!r}")
    return errors


def check_psalm_invitatory(lexicons: dict) -> list[str]:
    import yaml

    texts = lexicons["psalms.yaml"]
    book = yaml.load((CRATE / "data" / "books" / "psalm.yaml").read_text(), Loader=yaml.CSafeLoader)
    entries = []
    for number in ("94", "94c"):
        identifier = book["offices"][number]["slots"]["raw"]
        entries.append(texts[identifier]["content"]["zhs-ecu"])
    first, second = entries
    placeholders = [i for i, node in enumerate(first) if not re.match(r"^94:\d+", node.get("text", ""))]
    if placeholders != [2, 5, 8, 11, 14] or len(first) != 15 or len(second) != 10:
        return ["Psalm 94: invitatory insertion placeholders changed"]
    numbered = [node for i, node in enumerate(first) if i not in placeholders]
    if numbered != second:
        return ["Psalm 94 and 94c: numbered Chinese rows differ"]
    return []


def self_test(records: list[dict], lexicons: dict, source: dict) -> None:
    record = next(item for item in records if item["id"].startswith("psalm.1-"))
    field = record["fields"][1]
    def changed(replacement):
        corpus = deepcopy({"psalms.yaml": lexicons["psalms.yaml"]})
        node = corpus["psalms.yaml"][record["id"]]["content"]["zhs-ecu"][field["node_index"]]
        node["text"] = replacement(node["text"])
        return check_record(record, corpus, source)
    assert changed(lambda value: value.replace("这人便为有福", "")), "missing blessing was not detected"
    assert changed(lambda value: value + "这人便为有福"), "duplicated clause was not detected"
    assert changed(lambda value: value.replace("昼夜思想", "日夜思考")), "paraphrase was not detected"
    assert changed(lambda value: value + "*"), "unexpected Chinese asterisk was not detected"
    assert not changed(lambda value: value.replace("，", "；").replace("！", "。")), "punctuation modernization was rejected"
    corpus = deepcopy({"psalms.yaml": lexicons["psalms.yaml"]})
    key = next(key for key, entry in corpus["psalms.yaml"].items() if key.startswith("psalm.94-") and len(entry["content"]["zhs-ecu"]) == 15)
    corpus["psalms.yaml"][key]["content"]["zhs-ecu"].pop(2)
    assert check_psalm_invitatory(corpus), "missing invitatory placeholder was not detected"


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="also verify that deliberate text/structure corruptions are detected")
    options = parser.parse_args()
    document = json.loads(AUDIT.read_text(encoding="utf-8"))
    source = load_source()["verses"]
    records = document["records"]
    lexicons = {name: load_lexicon(name) for name in {field["file"] for record in records for field in record["fields"]}}
    errors = []
    for record in records:
        errors.extend(check_record(record, lexicons, source))
    for texts in lexicons.values():
        for identifier, entry in texts.items():
            for language in ("zhs", "zhs-ecu"):
                for node in entry.get("content", {}).get(language, []):
                    if any(isinstance(value, str) and "*" in value for value in node.values()):
                        errors.append(f"{identifier}: unexpected asterisk in {language}")
    errors.extend(check_psalm_invitatory(lexicons))
    for ref, text in source.items():
        if "?" in text or "\ufffd" in text:
            errors.append(f"{ref}: source still contains a broken character")
    psalm_ids = {record["id"] for record in records if record.get("chant_passage")}
    if len(psalm_ids) != 201:
        errors.append(f"expected all 151 psalm records and 50 biblical canticles, found {len(psalm_ids)}")
    if errors:
        for error in errors[:60]:
            print(error, file=sys.stderr)
        print(f"CUV audit failed: {len(errors)} errors", file=sys.stderr)
        return 1
    if options.self_test:
        self_test(records, lexicons, source)
    fields = sum(len(record["fields"]) for record in records)
    print(f"CUV audit passed: {len(records)} passages/fields, {fields} corpus fields, 151 psalm records, 50 biblical canticles.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
