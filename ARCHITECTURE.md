# Breviarium Architecture

This repository is a Cargo workspace with a root application crate and an
embedded data/resolver crate named `breviarium-data`. The data crate is the
liturgical engine. It embeds a normalized YAML corpus generated from Divinum
Officium and exposes typed Rust APIs that return structured office documents,
not HTML and not Divinum Officium strings.

The current target is the Roman 1960 office. The implementation is designed so
additional Divinum Officium profiles, languages, calendars, and office details
can be added without changing the public document model.

## Workspace Layout

The root `Cargo.toml` defines the workspace. The original root crate remains in
place under `src/`.

The data engine lives under:

```text
crates/breviarium-data/
  Cargo.toml
  src/lib.rs
  src/bin/import-divinum.rs
  src/bin/render-office.rs
  src/bin/check-office.rs
  data/
```

`breviarium-data` is a normal Rust library crate. Its binaries are maintenance
and verification tools:

- `import-divinum` imports a Divinum Officium checkout into normalized YAML.
- `render-office` renders one resolved hour as plain text for inspection.
- `check-office` resolves every date/hour in a year and reports structural
  failures, missing columns, unresolved nodes, and diagnostics.

## Data Embedding

Runtime never reads YAML from the filesystem. The crate embeds the whole
`crates/breviarium-data/data` tree with `include_dir!`.

At first use, `Breviarium::embedded()` lazily parses all embedded YAML files
into one `Catalog` stored in a `OnceLock`. There is no manifest. Loading walks
all embedded directories recursively and accepts any YAML document with a known
`doc_type`.

This means adding a data file is authoring-only work:

1. Put a valid YAML file anywhere under `data/`.
2. Give it a supported `doc_type`.
3. Rebuild the crate.

No Rust manifest update is needed.

## YAML Document Types

The resolver currently reads five document types.

`profile` defines a rubrical profile, currently `roman-1960`.

`rite` defines profile metadata. It exists so later profiles can carry their own
calendar and resolver configuration.

`office_skeleton` defines the ordered blocks for one profile/hour. A skeleton
does not contain prose. It says which resolver step produces each block.

`corpus_bundle` contains reusable multilingual text records. A corpus record is
the thing that should be translated: a psalm, hymn, collect, antiphon,
responsory, reading, rubric, or other semantic text unit. Each record is keyed
once and contains side-by-side language payloads:

```yaml
doc_type: corpus_bundle
texts:
  collect.deus-qui-salutis-a13f92:
    role: collect
    content:
      la:
        - type: text
          text: Deus, qui salútis ætérnæ...
        - type: prayer
          text: Qui tecum vivit...
        - type: response
          text: Amen.
      en:
        - type: text
          text: O God, Who, by the fruitful virginity...
        - type: prayer
          text: Who with thee liveth...
        - type: response
          text: Amen.
```

`source_bundle` contains liturgical source definitions. A source is an
observance, ordinary table, psalm, common, or martyrology day. Source sections
do not contain prose; they contain rank/rule metadata and references to corpus
texts:

```yaml
doc_type: source_bundle
sources:
  proper/sanctoral/01-01:
    metadata:
      rank:
        label: Duplex I classis
        value: 6.0
        common: Sancti/12-25
      rules:
        - kind: flag
          id: psalmi-dominica
          label: Psalmi Dominica
    sections:
      collect:
        role: collect
        text_id: collect.deus-qui-salutis-a13f92
      lauds-psalmody:
        role: psalmody
        text_id: psalmody.o-admirabile-commercium-f8c102
```

The directory shape remains author-friendly:

```text
data/corpus/*.yaml
data/sources/*.yaml
data/profiles/*.yaml
data/rites/*.yaml
```

Runtime semantics come from IDs, roles, source keys, metadata, and typed content
nodes, not from filenames.

## Corpus Texts

Each corpus text has:

- `role`: a semantic role such as `antiphon`, `collect`, `reading`, `psalmody`,
  `rubric`, or `martyrology_heading`.
- `content`: a map from language ID to typed content nodes.

Corpus texts intentionally do not contain provenance fields. They are reusable
text units. Multiple source sections may point to the same corpus text ID, and a
translation export should walk corpus texts rather than resolved offices.

## Source Sections

Source sections define which corpus text supplies a particular liturgical slot.
The source key is language-neutral, for example `proper/sanctoral/01-01`; the
section key names the slot, for example `collect` or `lauds-psalmody`.

At load time, the catalog derives an internal lookup index from source sections:

```text
(language, proper.sanctoral.01-01.collect) -> corpus collect content
```

This derived index is an implementation detail for the resolver. The YAML
source of truth remains normalized corpus texts plus source references.

## Content Nodes

The normalized YAML uses ordinary typed nodes:

- `text`: prose, lessons, hymns, and other ordinary text.
- `rubric`: explanatory directions.
- `marker`: structural labels.
- `heading`: a heading.
- `citation`: a citation.
- `versicle`: a versicle without the leading `V.`.
- `response`: a response without the leading `R.`.
- `short_response`: a short responsory response.
- `prayer`: a prayer or collect continuation.
- `blessing`: a blessing.
- `antiphon`: antiphon text.
- `psalm_ref`: a reference to a psalm or canticle.
- `psalmody`: an antiphon plus one or more psalm references.
- `table_row`: a labeled psalmody table row.
- `rank`: parsed rank metadata.
- `rule`: parsed rule tokens.

There are no runtime include nodes, macro nodes, transform nodes, or DO sigil
nodes. Divinum Officium `@`, `$`, `&`, range suffixes, duplicate sections, and
simple text substitutions are consumed by the importer.

The importer must not emit DO provenance lines, source sigils, raw include
syntax, HTML fragments, or text-transform syntax into YAML.

## Importer Responsibilities

`import-divinum` is the only code that understands Divinum Officium file syntax.
It performs these jobs:

1. Walk the DO checkout under `web/www`.
2. Decode text files and skip binary assets.
3. Classify files by service, language, and category.
4. Parse section headers where the service uses sectioned files.
5. Treat martyrology files as one raw section, because bracketed place names in
   prose are not section headers.
6. Build a section index for `@` includes.
7. Expand same-file and cross-file includes into typed content.
8. Expand known prayer macros such as `$Per Dominum` into text nodes.
9. Expand common macros such as `&Gloria` into text nodes.
10. Parse ranks into `rank` nodes.
11. Parse rules into `rule` tokens.
12. Parse psalmody rows into `psalmody`, `table_row`, and `psalm_ref` nodes.
13. Clean source-display artifacts such as HTML tags, braced source markers,
    legacy plus signs, and inline transform syntax.
14. Group language variants into multilingual corpus records.
15. Deduplicate identical multilingual text blocks.
16. Write source sections that reference corpus records by `text_id`.
17. Write deterministic YAML bundles.

The importer may be improved or replaced, but runtime must not regain DO string
expansion logic. Runtime may use structured metadata generated by the importer.

## Public API

The main entry point is:

```rust
let engine = breviarium_data::Breviarium::embedded()?;
let office = engine.resolve_office(request)?;
```

`OfficeRequest` contains:

- Gregorian `date`.
- `hour`.
- rubrical `profile`.
- requested `languages`.

`OfficeDocument` contains:

- computed `DateFacts`.
- selected principal observance.
- temporal and sanctoral candidates.
- commemorations.
- ordered output blocks.
- diagnostics.
- trace events.

Each `OfficeBlock` has one column per requested language. A column can be:

- `Resolved { nodes }`: the column has a structured document.
- `Missing { reason }`: the resolver could not build that whole column.

The design goal is that normal source gaps become `DocumentNode::Unresolved`
inside a resolved column rather than dropping the whole column. This is critical
for side-by-side languages: missing English text must be represented as missing
English text, not filled with Latin.

## Language Semantics

The first requested language is used for date/rank/rule selection when possible.
The current Roman 1960 corpus often has more complete Latin metadata, so date
selection can use Latin rank/rule records even when resolving English columns.

For each output column, language-neutral source keys are resolved through the
same source section and then into that column language inside the referenced
corpus text:

```text
proper/temporal/pasc1-0 -> sections.collect -> collect.foo -> content.en
```

No language fallback is performed. If the English bundle has no matching text,
the English column contains `Unresolved` for that component.

There is one same-language source-family fallback: common files with lettered
variants may fall back to the unlettered common in the same language. For
example, English may resolve `Commune/C2b-1.txt` through
`Commune/C2-1.txt` when English lacks the more specific file. This is not a
language fallback.

## Translation Export

New translations are produced from corpus texts, not resolved offices and not
source-section occurrences.

```sh
cargo run -p breviarium-data --bin export-translation -- \
  /tmp/to_translate.json \
  /tmp/to_translate.sidecar.json
```

`/tmp/to_translate.json` is a bare JSON array of Latin strings. The sidecar maps
each array index back to a corpus text ID and the content-node fields that
formed the string. The translation importer can use that sidecar to populate an
`en2` language payload in the same corpus records.

## Date Facts

`office_date_facts(date)` computes:

- Gregorian date.
- weekday.
- Gregorian Easter.
- temporal week key.
- temporal source stem.
- sanctoral key.

The implementation supports Gregorian dates from 1582-10-15 onward.

Temporal week keys follow the Divinum Officium stem conventions:

- `AdvN` for Advent.
- `NatDD` for Christmas season fixed dates.
- `EpiN` after Epiphany.
- `QuadpN` for pre-Lent.
- `QuadN` for Lent.
- `PascN` for Eastertide.
- `PentNN` after Pentecost.

The daily temporal stem is normally `{week}-{divinum_weekday}`. Sunday is
weekday `0`; Monday is `1`; Saturday is `6`.

## Source Selection

For a date, the resolver builds temporal and sanctoral candidates:

- temporal source candidates come from the temporal stem, with known DO suffix
  variants such as `Feria`, `o`, `t`, and `r`.
- sanctoral source candidates come from the fixed `MM-DD` key.

The principal observance is selected by rank. The higher-ranked candidate wins,
with temporal/sanctoral candidates retained for commemoration decisions.

For Lauds and Vespers, the non-principal candidate may become a commemoration
when its rank is positive and below the cutoff used by the current profile.

## Office Context

After selecting the principal, the resolver builds an `OfficeContext`.

The context stores:

- date facts.
- hour.
- profile.
- primary language.
- principal catalog key.
- temporal catalog key.
- weekly temporal catalog key.
- previous-day temporal catalog key.
- monthly Scripture catalog key.
- common catalog key.
- commemorations.
- parsed rule flags and rule values.
- Lauds variant.

Common catalog keys are discovered from structured `rank.common` and `rule`
source-reference tokens.

## Source Inheritance Rules

Principal/common/temporal inheritance is encoded in Rust because it is rubrical
logic, not text data.

`principal_sources()` returns:

1. principal source.
2. common source.

`inherited_sources()` returns:

1. principal source.
2. common source.
3. same-day temporal source.
4. weekly temporal source.
5. previous-day temporal source.

`collect_sources()` currently uses `inherited_sources()`.

`matins_lesson_sources()` returns:

1. principal source.
2. monthly Scripture source.
3. same-day temporal source.
4. weekly temporal source.
5. common source.

Commemoration sources include the commemoration source, its common, and for
temporal commemorations the weekly temporal fallback.

## Monthly Scripture Cycle

For August through November, Matins lessons 1-6 can come from monthly Scripture
files such as:

```text
083-0.txt
083-1.txt
```

The resolver computes the Sunday starting the current week. If that Sunday is in
August, September, October, or November, it builds the source stem:

```text
{month:02}{week_in_month}-{weekday}
```

For example, Sunday 2026-08-16 starts the third week of August, so Matins uses
`083-0.txt` for lessons 1-6 and `Pent12-0.txt` for the Gospel lessons.

## Rule Tokens

Rules are parsed into normalized tokens. Examples:

- `9 lectiones` -> flag `9-lectiones`.
- `Psalmi Dominica` -> flag `psalmi-dominica`.
- `Antiphonas horas` -> flag `antiphonas-horas`.
- `Minores sine Antiphona` -> flag `minores-sine-antiphona`.
- `Oratio Dominica` -> flag `oratio-dominica`.
- `Laudes 2` -> value `laudes = 2`.
- `vide Commune/C2b-1` -> source reference.

Runtime checks rules by normalized IDs.

## Skeleton Execution

The resolver loads the profile/hour skeleton, then executes each step in order.
Each step produces one block. Each block is resolved independently for each
requested language.

If a whole step fails for one language, that column is marked `Missing`.
Implementation policy is to avoid whole-column failure for ordinary source
gaps; missing subcomponents should become `DocumentNode::Unresolved`.

## Common Opening Rules

Openings are built from common prayers:

- Matins uses the Matins opening formula.
- Lauds, Vespers, and minor hours use the normal Deus in adjutorium opening.
- Seasonal Alleluia/Lent endings are selected from date facts.

## Matins Resolution

Matins blocks are:

1. opening.
2. invitatory.
3. hymn.
4. nocturns.

Invitatory:

1. Try principal/common `Invit`.
2. Fall back to the Matins special source by season.
3. Expand Psalm 94 with repeated invitatory markers.

Hymn:

1. Try principal/common `Hymnus Matutinum`.
2. Fall back to Matins special seasonal/day hymn.

Nocturn psalmody:

1. Try principal/common `Ant Matutinum`.
2. Fall back to `Psalterium/Psalmi/Psalmi matutinum.txt` day rows.
3. Expand psalm references into structured text.
4. Missing psalms become unresolved psalm nodes, not whole-column failure.

Lesson count:

1. If rule `9-lectiones` is present, use 9 lessons.
2. Else if principal/common has `Lectio4`, use 9 lessons.
3. Otherwise use 3 lessons.

For 3-lesson offices:

1. Put all psalmody in one nocturn.
2. Use the last available nocturn versicle group.
3. Resolve lessons 1-3.

For 9-lesson offices:

1. Split psalmody into groups of three.
2. After each group, resolve that nocturn's versicle.
3. Resolve lessons for that nocturn.

Lesson source order is principal, monthly Scripture, temporal, weekly temporal,
then common. Missing lessons become unresolved section nodes.

Responsories are resolved from the same lesson source order. If lesson 9 has no
responsory, the Te Deum formula is used.

## Lauds Resolution

Lauds blocks are:

1. opening.
2. psalmody.
3. chapter, hymn, and verse.
4. Benedictus.
5. preces.
6. collect and commemorations.
7. conclusion.

Psalmody:

1. Start from `Psalmi major.txt` for the weekday and Lauds variant.
2. If principal/common has `Ant Laudes`, merge those antiphons with ordinary
   psalms.
3. If principal/common has complete `psalmody`, use it.
4. Expand psalms and canticles.

Chapter, hymn, verse:

1. Try inherited sources for `Capitulum Laudes`, `Hymnus Laudes`, and
   `Versum 2`.
2. Fall back to Major Special Sunday or feria sections.
3. Missing subcomponents become unresolved nodes.

Benedictus:

1. Try principal/common `Ant 2`.
2. Fall back to the Major Special ferial Benedictus antiphon.
3. Expand Psalm 231 with the antiphon.

Preces:

The current implementation emits a structured omit marker where the Roman 1960
profile does not require preces for the resolved day.

Collects:

1. Emit `Domine exaudi`.
2. Emit `Oremus`.
3. Resolve collect from `collect_sources()`.
4. Use collect section candidates for the hour.
5. Resolve commemorations.

For Lauds and daytime hours the collect candidate order is:

1. `Oratio 2`.
2. `Oratio 3`.
3. `Oratio`.

## Prime Resolution

Prime blocks are:

1. opening.
2. hymn.
3. psalmody.
4. chapter and short responsory.
5. collect.
6. martyrology.
7. Pretiosa.
8. chapter office.
9. short reading.
10. conclusion.

Prime psalmody comes from `Psalmi minor.txt` and respects:

- Sunday psalms when rule `psalmi-dominica` applies.
- no antiphon when rule `minores-sine-antiphona` applies.
- proper minor-hour antiphons when available.
- Roman 1960 omission of optional psalms.

Martyrology uses the next calendar date. Latin uses `Martyrologium1960`; other
languages use `Martyrologium` when available. Martyrology files are imported as
raw text so bracketed prose never creates fake sections.

## Terce, Sext, and None Resolution

Minor hour blocks are:

1. opening.
2. hymn.
3. psalmody.
4. chapter, responsory, and verse.
5. collect.
6. conclusion.

Psalmody:

1. Select the hour row from `Psalmi minor.txt`.
2. Select Sunday rows when rule `psalmi-dominica` applies.
3. Apply proper antiphons from `Ant Tertia`, `Ant Sexta`, or `Ant Nona`.
4. Fall back to `Ant Laudes` or `Ant Vespera` when rule `antiphonas-horas`
   applies.
5. Remove optional psalms for Roman 1960.

Chapter/responsory/verse:

1. Try inherited sources for `Capitulum {hour}`.
2. Terce may fall back to `Capitulum Laudes`.
3. Fall back to Minor Special seasonal sections.
4. Resolve `Responsory Breve {hour}` or `Responsory breve {hour}`.
5. Resolve `Versum {hour}` or omit missing optional seasonal versicles.

## Vespers Resolution

Vespers blocks are:

1. opening.
2. psalmody.
3. chapter, hymn, and verse.
4. Magnificat.
5. preces.
6. collect and commemorations.
7. conclusion.

Psalmody uses the Vespers row from `Psalmi major.txt`, with proper antiphon
merging like Lauds.

Chapter, hymn, and verse:

1. Try inherited sources for `Capitulum Vespera 3`, `Capitulum Vespera`,
   `Capitulum Vespera 1`, then `Capitulum Laudes`.
2. Try inherited `Hymnus Vespera`.
3. Fall back to `Hymnus Day{weekday} Vespera` and, for Saturday, also
   `HymnusM Day6 Vespera`.
4. Try inherited `Versum 3`, `Versum 1`, then `Versum 2`.
5. Fall back to Major Special Sunday/feria Vespers sections.

The English DO corpus does not contain `Hymnus Day6 Vespera` or
`HymnusM Day6 Vespera`. The resolver represents that as an unresolved English
hymn node rather than falling back to Latin or substituting a different hymn.

Magnificat:

1. Try principal/common `Ant 3`.
2. Fall back to the Major Special ferial Magnificat antiphon.
3. Expand Psalm 232 with the antiphon.

For Vespers, collect candidate order is:

1. `Oratio 3`.
2. `Oratio 2`.
3. `Oratio`.

Commemorations at Vespers use `Ant 3`, `Versum 3`, and the Vespers collect
candidate order.

## Compline Resolution

Compline blocks are:

1. opening.
2. psalmody.
3. hymn.
4. chapter and responsory.
5. Nunc dimittis.
6. collect.
7. conclusion.
8. Marian antiphon.

Psalmody comes from the Compline row in `Psalmi minor.txt`.

The hymn is selected by season:

- Passiontide.
- Lent.
- Eastertide.
- ordinary Compline.

The final blessing tries both `benedictio Completorium Final` and
`Benedictio Completorium2` because the corpus uses both names.

## Missing Data Policy

The API distinguishes three situations:

1. A resolver bug or impossible request returns `DataError`.
2. A whole block/language cannot be built returns `OfficeColumnContent::Missing`.
3. A known subcomponent is absent returns `DocumentNode::Unresolved`.

Validation currently demonstrates zero hard failures and zero missing columns
for all 2026 Roman 1960 dates and all hours in Latin/English.

Remaining unresolved nodes in that sweep are known same-language corpus gaps,
primarily the missing English Saturday Vespers hymn sections in DO and the
missing English `Pent03-0` collect. These are intentionally not filled from
Latin because multi-language output must not fall back across languages.

## Verification Commands

Run formatting and compilation:

```sh
cargo fmt -p breviarium-data --check
cargo check -p breviarium-data
cargo doc -p breviarium-data --no-deps
```

Regenerate the corpus from a Divinum Officium checkout:

```sh
cargo run -q -p breviarium-data --bin import-divinum -- /tmp/divinum-officium-master
```

Render one hour:

```sh
cargo run -q -p breviarium-data --bin render-office -- 2026-01-01 lauds la en
```

Validate all dates and hours for a year:

```sh
cargo run -q -p breviarium-data --bin check-office -- 2026 la en
```

Audit for forbidden runtime DO syntax nodes:

```sh
rg -n "type: include|type: macro|transforms:|sigil:" crates/breviarium-data/data
rg -n "^\s*[@$&]" crates/breviarium-data/data/office/la crates/breviarium-data/data/office/en
```

Both `rg` audits should produce no matches.

## Extension Points

To add another profile:

1. Add a profile YAML document.
2. Add skeletons for each hour/profile combination.
3. Add any profile-specific source candidate rules in Rust.
4. Keep the public `OfficeDocument` model unchanged.

To add another language:

1. Import the DO language files.
2. Ensure `divinum_language_dir` maps the language ID to the DO directory.
3. Run the year checker with that language.
4. Represent missing language-specific texts as unresolved nodes, not fallback
   text from another language.

To add richer rendering:

1. Consume `OfficeDocument`.
2. Render `OfficeBlock` in order.
3. Render each language column side by side.
4. Treat `DocumentNode::Marker`, `Text`, and `Unresolved` as distinct node
   types.
5. Do not parse display text to recover structure.
