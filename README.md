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
