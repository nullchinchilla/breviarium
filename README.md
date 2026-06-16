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

`breviarium-data` embeds a semantic YAML migration of the Divinum Officium
Office, Mass, Martyrology, table, and chant corpus. The YAML is normalized into
reusable multilingual corpus texts plus liturgical source sections that refer to
those texts by ID. Its primary resolver API is `Breviarium::resolve_office`,
which returns structured Office documents for a date, hour, profile, and
language list. Requested languages are returned as side-by-side columns; the
resolver reports a missing column when a requested translation is unavailable
instead of silently falling back to Latin.

The embedded YAML lives in `crates/breviarium-data/data`. The catalog loader
recursively discovers every YAML file under that tree, so there is no manifest
to maintain. Normal runtime lookup does not read external files.

Regenerate the migrated corpus from a local Divinum Officium checkout:

```sh
cargo run -p breviarium-data --bin import-divinum -- /tmp/divinum-officium-master
```

Export Latin corpus strings for a new `en2` translation:

```sh
cargo run -p breviarium-data --bin export-translation -- \
  /tmp/to_translate.json \
  /tmp/to_translate.sidecar.json
```
