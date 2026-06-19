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
import re
import sys
from pathlib import Path

import yaml

# Keys within a content node that hold translatable natural-language prose.
TRANSLATABLE_KEYS = ("text", "antiphon")

HERE = Path(__file__).resolve().parent
CRATE = HERE.parent
LEXICON_DIR = CRATE / "data" / "lexicon"


def out_dir(lang: str) -> Path:
    """Per-language working dir holding ``latin.json`` and the translated array."""
    return CRATE / lang


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

    odir = out_dir(args.lang)
    odir.mkdir(parents=True, exist_ok=True)
    latin_path = odir / "latin.json"
    latin_path.write_text(
        json.dumps(unique, ensure_ascii=False, indent=0) + "\n", encoding="utf-8"
    )

    manifest = {
        "unique_strings": len(unique),
        "total_occurrences": total,
        "per_file_occurrences": per_file,
        "latin_json": str(latin_path.relative_to(CRATE)),
    }
    (odir / "manifest.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )

    print(f"wrote {latin_path}")
    print(f"  unique translatable strings : {len(unique)}")
    print(f"  total occurrences           : {total}")
    print(f"  dedup ratio                 : {len(unique)/total:.1%}" if total else "")
    return 0


def build_lang_nodes(la_nodes, mapping, stats):
    """Clone the ``la`` node list, replacing each prose field via the map.

    Returns ``(new_nodes, missing)`` where ``missing`` is the count of prose
    fields with no translation. Untranslated fields keep their Latin text."""
    new_nodes = copy.deepcopy(la_nodes)
    missing = 0
    for node in new_nodes:
        if not isinstance(node, dict):
            continue
        for key, val in list(node_strings(node)):
            repl = mapping.get(val)
            if repl is None:
                missing += 1
                stats["missing"] += 1
            else:
                node[key] = repl
                stats["applied"] += 1
    return new_nodes, missing


class _Dumper(yaml.SafeDumper):
    pass


def _repr_str(dumper, data):
    # Match the lexicon's house style: multiline prose uses literal `|` blocks.
    style = "|" if "\n" in data else None
    return dumper.represent_scalar("tag:yaml.org,2002:str", data, style=style)


_Dumper.add_representer(str, _repr_str)


def render_block(nodes, indent, lang="en2"):
    """Render a ``<lang>:`` block as YAML, indented to match the la/en columns."""
    body = yaml.dump(
        {lang: nodes},
        Dumper=_Dumper,
        allow_unicode=True,
        sort_keys=False,
        width=4096,
        default_flow_style=False,
    )
    pad = " " * indent
    return "".join(pad + line + "\n" for line in body.splitlines())


def cmd_apply(args) -> int:
    lang = args.lang
    translated = json.loads(Path(args.translation).read_text(encoding="utf-8"))
    # Two input shapes: a positional array parallel to latin.json (full run,
    # untranslated slots stay Latin), or a {latin: translation} dict (partial /
    # pilot — only entries whose every prose field is covered get a column).
    partial = isinstance(translated, dict)
    if partial:
        mapping = translated
    else:
        latin = json.loads((out_dir(lang) / "latin.json").read_text(encoding="utf-8"))
        if len(latin) != len(translated):
            print(
                f"ERROR: latin.json has {len(latin)} strings but "
                f"{args.translation} has {len(translated)}",
                file=sys.stderr,
            )
            return 1
        mapping = dict(zip(latin, translated))

    stats = {"applied": 0, "missing": 0}
    entries_done = 0
    entries_skipped = 0
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
            if "la" not in langs or lang in langs:
                entries_no_la += "la" not in langs
                continue
            indent = content.value[0][0].start_mark.column
            insert_line = content.value[-1][1].end_mark.line

            la_doc_nodes = doc_texts[key_node.value]["content"]["la"]
            lang_nodes, missing = build_lang_nodes(la_doc_nodes, mapping, stats)
            # In partial mode, only emit a column for fully-translated entries.
            if partial and missing:
                entries_skipped += 1
                continue
            inserts.append((insert_line, render_block(lang_nodes, indent, lang)))
            entries_done += 1

        for insert_line, block in sorted(inserts, reverse=True):
            lines.insert(insert_line, block)

        if not args.dry_run and inserts:
            path.write_text("".join(lines), encoding="utf-8")
            print(f"updated {path.name}: +{len(inserts)} {lang} blocks")

    print(f"entries given {lang:<10}: {entries_done}")
    print(f"entries partial (skip)   : {entries_skipped}")
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


# A leading `chapter:verse` marker (`1:46`, `127:3a`), matching the resolver's
# `has_verse_prefix` in resolve.rs.
_VERSE_PREFIX = re.compile(r"^(\d+:\d+[a-z]?)(?=\s|$)")


def restore_verse_prefixes(en2_nodes, la_nodes) -> bool:
    """Re-attach `chapter:verse` markers the translation dropped from verses.

    The resolver tells a psalm/canticle verse from its title/citation preamble
    by the leading `N:M` number (``has_verse_prefix`` / ``psalm_nodes`` in
    resolve.rs). Some translated lines lost that marker, so whole canticles
    (the Magnificat, Benedictus, Isaian canticles, …) were mis-rendered as
    citations with no verse text. en2 mirrors la node-for-node and line-for-line,
    so copy each missing prefix verbatim from the parallel Latin line. Mutates
    ``en2_nodes`` in place; returns whether anything changed."""
    touched = False
    for i, node in enumerate(en2_nodes):
        if i >= len(la_nodes):
            break
        la_field = node_prose(la_nodes[i])
        en2_field = node_prose(node)
        if not la_field or not en2_field:
            continue
        la_lines = la_field[1].split("\n")
        en2_lines = en2_field[1].split("\n")
        if len(la_lines) != len(en2_lines):
            continue  # translation reflowed this block; can't align lines safely
        changed = False
        for j, (la_line, en2_line) in enumerate(zip(la_lines, en2_lines)):
            match = _VERSE_PREFIX.match(la_line.strip())
            if not match:
                continue
            prefix = match.group(1)
            existing = _VERSE_PREFIX.match(en2_line.strip())
            if existing and existing.group(1) == prefix:
                continue
            stripped = en2_line.lstrip()
            lead = en2_line[: len(en2_line) - len(stripped)]
            en2_lines[j] = f"{lead}{prefix} {stripped}"
            changed = True
        if changed:
            node[en2_field[0]] = "\n".join(en2_lines)
            touched = True
    return touched


def cmd_fixups(args) -> int:
    lang = args.lang
    only = getattr(args, "only", "all")
    # The hymn (→en) and Salve Regina corrections are English-specific; the
    # verse-prefix restoration applies to any column that mirrors `la` 1:1.
    en_family = lang.startswith("en")
    do_hymns = en_family and only in ("all", "hymns")
    do_salve = en_family and only in ("all", "salve")
    do_verses = only in ("all", "verses")
    hymns = salves = verses = 0
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
            if lang not in langs:
                continue
            entry = doc_texts[key_node.value]
            cont = entry["content"]
            col_nodes = cont[lang]
            new_nodes = None

            if do_hymns and entry.get("role") == "hymn" and "en" in cont:
                # change 2: hymns use the metrical en translation verbatim.
                new_nodes = copy.deepcopy(cont["en"])
                if new_nodes != col_nodes:
                    hymns += 1
            elif entry.get("role") != "hymn" or "en" not in cont:
                # the lang column mirrors la 1:1, so corrections key off the
                # parallel la node's prose.
                la_nodes = cont.get("la") or []
                candidate = copy.deepcopy(col_nodes)
                touched = False
                # change 3: Salve Regina → traditional translation.
                if do_salve:
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
                        salves += 1
                # change 4: restore dropped chapter:verse markers so psalm and
                # canticle verses are not mis-rendered as citations.
                if do_verses and restore_verse_prefixes(candidate, la_nodes):
                    verses += 1
                    touched = True
                if touched:
                    new_nodes = candidate

            if new_nodes is None or new_nodes == col_nodes:
                continue
            key_n = next(k for k, _ in content.value if k.value == lang)
            indent = key_n.start_mark.column
            edits.append(
                (key_n.start_mark.line, langs[lang].end_mark.line,
                 render_block(new_nodes, indent, lang))
            )

        for start, end, block in sorted(edits, reverse=True):
            lines[start:end] = [block]

        if edits and not args.dry_run:
            path.write_text("".join(lines), encoding="utf-8")
            print(f"updated {path.name}: {len(edits)} {lang} blocks rewritten")

    print(f"hymn {lang} blocks set to en : {hymns}")
    print(f"Salve Regina {lang} rewritten: {salves}")
    print(f"verse-prefixes restored in: {verses}")
    if args.dry_run:
        print("(dry run — no files written)")
    return 0


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--lang",
        default="en2",
        help="target language column / working dir (default: en2; e.g. zh)",
    )
    sub = parser.add_subparsers(dest="cmd", required=True)

    p_ex = sub.add_parser("extract", help="write <lang>/latin.json from the lexicon")
    p_ex.set_defaults(func=cmd_extract)

    p_ap = sub.add_parser(
        "apply", help="inject <lang> column from a translated array or {latin:tr} dict"
    )
    p_ap.add_argument(
        "translation",
        help="JSON array parallel to latin.json, or a {latin: translation} dict (partial)",
    )
    p_ap.add_argument("--dry-run", action="store_true", help="don't write files")
    p_ap.set_defaults(func=cmd_apply)

    p_fx = sub.add_parser(
        "fixups",
        help="post-apply en2 corrections: hymns use en, Salve Regina traditional, "
        "restore dropped verse numbers",
    )
    p_fx.add_argument("--dry-run", action="store_true", help="don't write files")
    p_fx.add_argument(
        "--only",
        choices=("all", "hymns", "salve", "verses"),
        default="all",
        help="apply only one correction (default: all)",
    )
    p_fx.set_defaults(func=cmd_fixups)

    args = parser.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
