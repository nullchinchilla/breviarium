#!/usr/bin/env python3
"""en2 translation pipeline for the breviarium lexicon.

The lexicon (``crates/breviarium-data/data/lexicon/*.yaml``) stores every text
unit as a multilingual entry::

    <text-id>:
      role: <role>
      content:
        la: [ <node>, ... ]
        en: [ <node>, ... ]

Each ``<node>`` is a small mapping. The only keys that ever hold
natural-language prose are ``text`` (all the text-like node types) and
``antiphon`` (psalmody nodes). Everything else (``label`` = verse numbers,
``number`` / ``start`` / ``end`` psalm refs, ...) is structural.

This tool has two commands:

  extract
      Walk the ``la`` column of every entry, collect the unique, non-empty
      translatable strings, and write them as a JSON array of strings
      (``latin.json``) — the input format the translation system expects.

  apply
      Read ``latin.json`` and the parallel translated array (``en2.json``),
      build a ``{latin: en2}`` map, then re-walk the lexicon and inject an
      ``en2`` content column built by cloning the ``la`` nodes and replacing
      their prose with the translation. Idempotent and re-runnable after a
      re-import, because it keys on the Latin string rather than position.

The walk order is fully deterministic (sorted files, document order within a
file), so extract and apply agree.
"""

from __future__ import annotations

import argparse
import copy
import json
import sys
from pathlib import Path

import yaml

# Keys within a content node that hold translatable natural-language prose.
TRANSLATABLE_KEYS = ("text", "antiphon")

HERE = Path(__file__).resolve().parent
CRATE = HERE.parent
LEXICON_DIR = CRATE / "data" / "lexicon"
OUT_DIR = CRATE / "en2"


def lexicon_files() -> list[Path]:
    return sorted(LEXICON_DIR.glob("*.yaml"))


def load(path: Path):
    with path.open(encoding="utf-8") as fh:
        return yaml.safe_load(fh)


def iter_la_nodes(doc):
    """Yield every node mapping in the ``la`` column, in document order."""
    if not doc:
        return
    texts = doc.get("texts") or {}
    for entry in texts.values():
        content = (entry or {}).get("content") or {}
        for node in content.get("la") or []:
            if isinstance(node, dict):
                yield node


def node_strings(node: dict):
    """Yield (key, value) for each translatable, non-empty prose field."""
    for key in TRANSLATABLE_KEYS:
        val = node.get(key)
        if isinstance(val, str) and val.strip():
            yield key, val


def cmd_extract(args) -> int:
    seen: dict[str, None] = {}  # ordered set: first-seen order
    total = 0
    per_file = {}
    for path in lexicon_files():
        doc = load(path)
        count = 0
        for node in iter_la_nodes(doc):
            for _key, val in node_strings(node):
                total += 1
                count += 1
                if val not in seen:
                    seen[val] = None
        per_file[path.name] = count

    unique = list(seen.keys())

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    latin_path = OUT_DIR / "latin.json"
    latin_path.write_text(
        json.dumps(unique, ensure_ascii=False, indent=0) + "\n", encoding="utf-8"
    )

    manifest = {
        "unique_strings": len(unique),
        "total_occurrences": total,
        "per_file_occurrences": per_file,
        "latin_json": str(latin_path.relative_to(CRATE)),
    }
    (OUT_DIR / "manifest.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )

    print(f"wrote {latin_path}")
    print(f"  unique translatable strings : {len(unique)}")
    print(f"  total occurrences           : {total}")
    print(f"  dedup ratio                 : {len(unique)/total:.1%}" if total else "")
    return 0


def build_en2_nodes(la_nodes, mapping, stats):
    """Clone the ``la`` node list, replacing each prose field via the map."""
    new_nodes = copy.deepcopy(la_nodes)
    for node in new_nodes:
        if not isinstance(node, dict):
            continue
        for key, val in list(node_strings(node)):
            repl = mapping.get(val)
            if repl is None:
                stats["missing"] += 1
            else:
                node[key] = repl
                stats["applied"] += 1
    return new_nodes


class _Dumper(yaml.SafeDumper):
    pass


def _repr_str(dumper, data):
    # Match the lexicon's house style: multiline prose uses literal `|` blocks.
    style = "|" if "\n" in data else None
    return dumper.represent_scalar("tag:yaml.org,2002:str", data, style=style)


_Dumper.add_representer(str, _repr_str)


def render_block(en2_nodes, indent):
    """Render an ``en2:`` block as YAML, indented to match the la/en columns."""
    body = yaml.dump(
        {"en2": en2_nodes},
        Dumper=_Dumper,
        allow_unicode=True,
        sort_keys=False,
        width=4096,
        default_flow_style=False,
    )
    pad = " " * indent
    return "".join(pad + line + "\n" for line in body.splitlines())


def cmd_apply(args) -> int:
    latin = json.loads((OUT_DIR / "latin.json").read_text(encoding="utf-8"))
    en2 = json.loads(Path(args.translation).read_text(encoding="utf-8"))
    if len(latin) != len(en2):
        print(
            f"ERROR: latin.json has {len(latin)} strings but "
            f"{args.translation} has {len(en2)}",
            file=sys.stderr,
        )
        return 1
    mapping = dict(zip(latin, en2))

    stats = {"applied": 0, "missing": 0}
    entries_done = 0
    entries_no_la = 0
    for path in lexicon_files():
        text = path.read_text(encoding="utf-8")
        lines = text.splitlines(keepends=True)
        if not lines:
            continue
        root = yaml.compose(text)
        doc = yaml.safe_load(text)
        texts_node = next(
            (v for k, v in root.value if k.value == "texts"), None
        )
        if texts_node is None:
            continue
        doc_texts = doc.get("texts") or {}

        # Collect (insert_at_line, rendered_block) then splice bottom-up so
        # earlier insertions don't shift later line numbers.
        inserts = []
        for key_node, entry_node in texts_node.value:
            content = next(
                (v for k, v in entry_node.value if k.value == "content"), None
            )
            if content is None or not content.value:
                continue
            langs = {k.value: v for k, v in content.value}
            if "la" not in langs or "en2" in langs:
                entries_no_la += "la" not in langs
                continue
            indent = content.value[0][0].start_mark.column
            insert_line = content.value[-1][1].end_mark.line

            la_doc_nodes = doc_texts[key_node.value]["content"]["la"]
            en2_nodes = build_en2_nodes(la_doc_nodes, mapping, stats)
            inserts.append((insert_line, render_block(en2_nodes, indent)))
            entries_done += 1

        for insert_line, block in sorted(inserts, reverse=True):
            lines.insert(insert_line, block)

        if not args.dry_run:
            path.write_text("".join(lines), encoding="utf-8")
            print(f"updated {path.name}: +{len(inserts)} en2 blocks")

    print(f"entries given en2        : {entries_done}")
    print(f"entries with no la (skip): {entries_no_la}")
    print(f"prose fields translated  : {stats['applied']}")
    print(f"prose fields left Latin   : {stats['missing']}")
    if args.dry_run:
        print("(dry run — no files written)")
    return 0


# --- fixups: targeted post-apply corrections to the en2 column ----------------

# Traditional English of the Salve Regina, line-aligned to the Latin.
_SALVE_LINES = [
    "Hail, holy Queen, Mother of mercy,",
    "our life, our sweetness, and our hope.",
    "To thee do we cry, poor banished children of Eve.",
    "To thee do we send up our sighs, mourning and weeping",
    "in this valley of tears.",
    "Turn then, most gracious Advocate,",
    "thine eyes of mercy toward us,",
    "and after this our exile show unto us",
    "the blessed fruit of thy womb, Jesus.",
    "O clement, O loving, O sweet Virgin Mary.",
]
_SALVE_PREFIX = "Salve, Regína, mater misericórdiæ"


def salve_regina_en2(latin: str) -> str:
    """Traditional Salve Regina, matching the structure of the Latin variant."""
    if "~" in latin:  # variant with trailing `~` line markers
        return "\n".join(line + " ~" for line in _SALVE_LINES) + "\n~"
    base = "\n".join(_SALVE_LINES)
    if "(Allelúia.)" in latin:
        return base + "\n\n(Alleluia.)"
    return base


def node_prose(node):
    """Return (key, value) of a node's translatable prose field, or None."""
    if isinstance(node, dict):
        for key in TRANSLATABLE_KEYS:
            val = node.get(key)
            if isinstance(val, str):
                return key, val
    return None


def cmd_fixups(args) -> int:
    hymns = salves = 0
    for path in lexicon_files():
        text = path.read_text(encoding="utf-8")
        lines = text.splitlines(keepends=True)
        if not lines:
            continue
        root = yaml.compose(text)
        doc = yaml.safe_load(text)
        texts_node = next((v for k, v in root.value if k.value == "texts"), None)
        if texts_node is None:
            continue
        doc_texts = doc.get("texts") or {}

        edits = []  # (start_line, end_line, rendered_block)
        for key_node, entry_node in texts_node.value:
            content = next(
                (v for k, v in entry_node.value if k.value == "content"), None
            )
            if content is None or not content.value:
                continue
            langs = {k.value: v for k, v in content.value}
            if "en2" not in langs:
                continue
            entry = doc_texts[key_node.value]
            cont = entry["content"]
            en2_nodes = cont["en2"]
            new_nodes = None

            if entry.get("role") == "hymn" and "en" in cont:
                # change 2: hymns use the metrical en translation verbatim.
                new_nodes = copy.deepcopy(cont["en"])
                if new_nodes != en2_nodes:
                    hymns += 1
            else:
                # change 3: Salve Regina → traditional translation. en2 nodes
                # mirror la 1:1, so match on the parallel la node's prose.
                la_nodes = cont.get("la") or []
                candidate = copy.deepcopy(en2_nodes)
                touched = False
                for i, node in enumerate(candidate):
                    if i >= len(la_nodes):
                        break
                    la_field = node_prose(la_nodes[i])
                    en2_field = node_prose(node)
                    if (
                        la_field
                        and en2_field
                        and la_field[1].startswith(_SALVE_PREFIX)
                    ):
                        node[en2_field[0]] = salve_regina_en2(la_field[1])
                        touched = True
                if touched:
                    new_nodes = candidate
                    salves += 1

            if new_nodes is None or new_nodes == en2_nodes:
                continue
            key_n = next(k for k, _ in content.value if k.value == "en2")
            indent = key_n.start_mark.column
            edits.append(
                (key_n.start_mark.line, langs["en2"].end_mark.line,
                 render_block(new_nodes, indent))
            )

        for start, end, block in sorted(edits, reverse=True):
            lines[start:end] = [block]

        if edits and not args.dry_run:
            path.write_text("".join(lines), encoding="utf-8")
            print(f"updated {path.name}: {len(edits)} en2 blocks rewritten")

    print(f"hymn en2 blocks set to en : {hymns}")
    print(f"Salve Regina en2 rewritten: {salves}")
    if args.dry_run:
        print("(dry run — no files written)")
    return 0


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_ex = sub.add_parser("extract", help="write latin.json from the lexicon")
    p_ex.set_defaults(func=cmd_extract)

    p_ap = sub.add_parser("apply", help="inject en2 column from a translated array")
    p_ap.add_argument(
        "translation", help="path to the translated JSON array (parallel to latin.json)"
    )
    p_ap.add_argument("--dry-run", action="store_true", help="don't write files")
    p_ap.set_defaults(func=cmd_apply)

    p_fx = sub.add_parser(
        "fixups",
        help="post-apply en2 corrections: hymns use en, Salve Regina traditional",
    )
    p_fx.add_argument("--dry-run", action="store_true", help="don't write files")
    p_fx.set_defaults(func=cmd_fixups)

    args = parser.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
