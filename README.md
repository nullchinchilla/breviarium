# Breviarium

Breviarium is a Rust/Dioxus application with an embedded liturgical data crate.

The root package remains the Dioxus app. The workspace also contains
`crates/breviarium-data`, a pure Rust crate that embeds YAML data at compile time
and exposes a typed lookup API for liturgical texts.

## Development

Run all tests:

```sh
cargo test --workspace
```

Build the data crate documentation:

```sh
RUSTDOCFLAGS="-D warnings" cargo doc -p breviarium-data --no-deps
```

Run the Dioxus dev server:

```sh
dx serve
```

Open http://localhost:8080.

## Data Crate

`breviarium-data` embeds a semantic YAML corpus of the Office, Mass,
Martyrology, table, and chant texts. The YAML is normalized into reusable
multilingual corpus texts plus liturgical source sections that refer to those
texts by ID. Its primary resolver API is `Breviarium::resolve_office`, which
returns structured Office documents for a date, hour, profile, and language
list. Requested languages are returned as side-by-side columns; the resolver
reports a missing column when a requested translation is unavailable instead of
silently falling back to Latin.

The embedded YAML in `crates/breviarium-data/data` is the source of truth. The
catalog loader recursively discovers every YAML file under that tree, so there
is no manifest to maintain. Normal runtime lookup does not read external files.

### English psalter

The numbered psalms in the active `en` column use the
[Catholic Public Domain Version](https://www.sacredbible.org/catholic/OT-21_Psalms.htm).
CPDV wording and punctuation are divided into the Latin breviary's verse rows,
with its chant marks, embedded verse references, and acrostic headings retained.
Publisher superscriptions and the closing colophon of Psalm 71 are excluded
from liturgical recitation. Alleluia is omitted wherever the corresponding Latin
psalm text omits it, including the CPDV endings of Psalms 147–150.

The publisher's original verses are retained in
`crates/breviarium-data/cpdv/psalms.json`; `source-exclusions.json` records the
exact excluded material. Check all 150 psalms and both Psalm 94 variants with
Python 3 and PyYAML:

```sh
python3 crates/breviarium-data/tools/cpdv_psalms.py
```

The check compares complete psalm bodies, including punctuation, across verse
boundaries and verifies that the English notation matches Latin. It detects
missing or repeated clauses even when the verse labels and node counts agree.

### `en2` translation

The `en2` column is produced by `crates/breviarium-data/tools/en2.py`, which
walks the Latin (`la`) column of the lexicon. It keys translations on the Latin
source string, so `apply` is idempotent and re-runnable.

```sh
# 1. Extract the unique Latin strings as a JSON array for the translator:
python3 crates/breviarium-data/tools/en2.py extract
#    → crates/breviarium-data/en2/latin.json

# 2. Translate that array (preserving length and order), then inject the
#    en2 column back into the lexicon:
python3 crates/breviarium-data/tools/en2.py apply crates/breviarium-data/en2/english.json
```
