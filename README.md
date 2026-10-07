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

Translations are stored directly in the multilingual YAML lexicon.

### Ecumenical Chinese Scripture

The `zhs-ecu` column follows the older public-domain Chinese Union Version
(CUV), with our own punctuation modernization. It does not use the copyrighted
New Punctuation or Revised CUV. Biblical wording follows CUV; 耶和华 becomes
主 or 神 according to the Latin Dominus or Deus. The Latin liturgical text
determines passage boundaries, omitted Alleluias, and necessary adaptations.
Deuterocanonical passages and Latin additions without a canonical CUV parallel
use original translations following CUV conventions.
Instrumental “per” uses 借, including prayer conclusions. Both Chinese columns
omit asterisks; the remaining liturgical notation is retained.
The Glory Be begins “愿荣耀归于父、子、圣灵。” Prayer conclusions use
“共生共治，独一的神，世世无尽”, with pronouns matching the Latin address.

The retained source, documented source repairs, references, and reviewed
adaptations are in `crates/breviarium-data/cuv`. Check the reviewed wording,
verse labels, and chant divisions offline with Python 3 and PyYAML:

```sh
python3 crates/breviarium-data/tools/cuv_audit.py --self-test
```
