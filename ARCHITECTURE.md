# Breviarium Architecture

Last updated: 2026-06-15

This document defines the target architecture for a Rust implementation of a
Divine Office engine. It is intentionally specific enough that an intern or an
LLM should be able to implement the system without guessing where liturgical
decisions belong.

The design does not preserve the Divinum Officium runtime architecture. The old
project is useful as a corpus, comparison oracle, and migration source, but the
Rust system should be built around typed data, explicit rubrical decisions, and
rendering as the final step.

## Goals

The system must:

- Compute the canonical structure and text of any requested office hour for a
  given date, rubric profile, calendar, language set, and options.
- Keep liturgical decisions out of renderers.
- Keep arbitrary executable logic out of source data.
- Preserve enough provenance to explain why every text appeared.
- Support multiple rubrical profiles, beginning with Roman 1960.
- Support Latin and translations without duplicating calendar logic.
- Support local calendars, commons, temporal cycles, transfers, vigils, octaves,
  commemorations, variants, and psalter schemes.
- Produce deterministic results from immutable input.
- Be testable against known examples and, during migration, against Divinum
  Officium output.

## Non-Goals

The first version should not:

- Implement a generic user-programmable rules engine.
- Interpret the original Divinum Officium text files at runtime.
- Put HTML or CSS in the liturgical core.
- Let YAML contain arbitrary procedural logic.
- Use mutable global state for the current date, hour, winner, rank, or language.
- Conflate the calendar decision with text rendering.

## System Shape

The application has five layers:

1. Source data
2. Data loader, validator, and indexer
3. Liturgical core engine
4. Server/API
5. UI/renderers

Only the core engine decides liturgical structure. Renderers display a finished
`OfficeDocument`.

Recommended crate layout:

```text
crates/
  breviarium-core/       Pure date, calendar, rubrics, resolution, document model
  breviarium-data/       YAML loader, validators, typed catalog structs, indexes
  breviarium-import/     Optional migration/import tools for legacy sources
  breviarium-server/     HTTP/API integration, caching, request normalization
  breviarium-render/     HTML, plain text, JSON, and future EPUB/PDF renderers
  breviarium-cli/        Debugging, validation, corpus comparison, snapshots

data-src/
  profiles/
  calendars/
  observances/
  commons/
  ordinary/
  psalter/
  texts/
  chants/
  citations/
  imports/

debug-output/
  catalog.normalized.json
  validation-report.json
```

The current Dioxus app can initially host `breviarium-server`,
`breviarium-render`, and the web UI in the main crate, but the domain model
should be designed as if `breviarium-core` is independently reusable.

## Core Principle

The runtime flow is:

```text
OfficeRequest
  -> DateFacts
  -> CandidateSet
  -> AdjustedCandidateSet
  -> ResolvedDay
  -> HourPlan
  -> OfficeDocument
  -> RenderedOutput
```

Each step takes immutable input and returns a new value plus trace records. No
step should mutate shared global state.

The only layer that emits HTML is the renderer. The only layers that decide
rank, precedence, commemoration, transfer, or hour extent are the rubrical
layers.

## Runtime Request

The normalized request must contain all inputs needed for deterministic output:

```text
OfficeRequest
  date: GregorianDate
  hour: Hour
  profile_id: ProfileId
  calendar_scope: CalendarScope
  languages: Vec<LanguageId>
  options: OfficeOptions
```

Fields:

- `date`: Calendar date, not a timestamp. Timezone only matters when converting
  "today" into a date.
- `hour`: One of Matins, Lauds, Prime, Terce, Sext, None, Vespers, Compline.
- `profile_id`: Example: `roman-1960`.
- `calendar_scope`: General calendar plus optional local calendar overlays.
- `languages`: Usually `["la", "en"]`.
- `options`: Explicit flags such as priest-led versicles, psalter variant, hymn
  variant, display rubrics, chant inclusion, and comparison mode.

Request normalization belongs in the server or CLI, not in the core. The core
must reject incomplete requests.

## DateFacts

`DateFacts` is the pure calendar/date layer. It answers questions that do not
depend on which observance wins.

It must include:

- Gregorian year, month, day.
- Day of week.
- Leap year flag.
- Fixed-date key, such as `06-15`.
- Easter date for the year.
- Movable season.
- Liturgical week key.
- Temporal weekday key.
- Paschaltide flag.
- Advent week/day.
- Septuagesima/Lent/Passiontide/Holy Week facts.
- Easter octave and Pentecost octave facts where applicable by profile.
- Ember day facts.
- Rogation day facts.
- Christmas/Epiphany season facts.
- "After Epiphany" and "After Pentecost" displaced-week information.

Date calculation must not inspect sanctoral observances. It may inspect the
profile for calendar-era differences, such as whether a profile has a Pentecost
octave, but it must not decide occurrence.

Easter must be computed with a documented Gregorian algorithm and unit-tested for
all years supported by the app.

## Source Data Philosophy

Source data should be human-editable YAML or a similarly structured format.

There is no required separate compilation phase. At application startup, the data
loader parses source YAML, validates it, resolves inheritance, checks references,
and builds a typed in-memory `Catalog`. The resolver receives this catalog and
never walks untyped YAML nodes directly.

Walking raw YAML for every request is allowed only for quick prototypes. It is
not the target design because it delays validation until a user hits a date,
makes reference errors harder to diagnose, and pushes stringly typed lookup code
into the liturgical resolver.

An optional prebuilt catalog bundle may be added later for faster production
startup, but it must be an optimization of the same loader output, not a separate
source of truth.

Data may contain structured variants, for example "this text is used in
Paschaltide." Data may not contain executable rules such as "if rank is less
than X and the winner is a Sunday, mutate commemorations."

The boundary is:

```text
Data says what exists and what simple variants are available.
Rust decides what applies and why.
```

## Source Data Directory Layout

Recommended layout:

```text
data-src/
  profiles/
    roman-1960.yaml
    divino-afflatu-1954.yaml

  calendars/
    general/
      roman.yaml
    local/
      urbis.yaml
      passau.yaml

  observances/
    temporal/
      after-pentecost.yaml
      advent.yaml
      lent.yaml
    sanctoral/
      06/15-ss-vitus-modestus-crescentia.yaml
    movable/
      easter.yaml
      corpus-christi.yaml

  commons/
    martyrs.yaml
    apostles.yaml
    confessors.yaml
    bvm.yaml

  ordinary/
    roman/
      lauds.yaml
      vespers.yaml
      matins.yaml

  psalter/
    roman-weekly.yaml
    monastic.yaml
    bea.yaml

  texts/
    la/
      prayers.yaml
      psalms.yaml
      hymns.yaml
    en/
      prayers.yaml
      psalms.yaml
      hymns.yaml

  citations/
    roman-1960.yaml
```

Files should be organized for editors. The loader may build very different
in-memory indexes optimized for runtime lookup.

## Identifiers

All domain objects must have stable IDs. Do not use translated titles as keys.

Examples:

```text
profile: roman-1960
calendar: general-roman
observance: ss-vitus-modestus-crescentia
temporal: after-pentecost-week-3-monday
common: several-martyrs
text: collect.ss-vitus-modestus-crescentia
psalter: roman-weekly
slot: lauds.psalmody
```

ID rules:

- Lowercase ASCII.
- Hyphen-separated.
- Stable across languages.
- Never include rank, year, or title words that may change by translation.
- Renames require an alias entry so old snapshots remain explainable.

## Profiles

A profile defines the rubrical universe. It is not just a version string.

Example fields:

```yaml
id: roman-1960
rite: roman
calendar_family: roman-general
rank_system: roman-1960
ordinary: roman
psalter: roman-weekly-1960
rubrics_module: roman_1960
default_languages: [la, en]
supports:
  octaves: limited
  vigils: limited
  commemorations: restricted
```

Profiles define:

- Rank vocabulary.
- Rank ordering.
- Calendar inheritance.
- Active temporal cycle.
- Active psalter scheme.
- Ordinary templates.
- Which Rust rubrics module to use.
- Which structured data dimensions are allowed.
- Which options are legal.

Profiles do not contain arbitrary decision logic. They select typed Rust logic.

## Observance Data

An observance is a possible liturgical object: a saint, feast, feria, vigil,
octave day, Sunday, temporal day, or votive office.

Conceptual schema:

```yaml
id: ss-vitus-modestus-crescentia
kind: sanctoral
date:
  fixed: 06-15
title:
  la: "Ss. Viti, Modesti atque Crescentiae Martyrum"
  en: "Ss. Vitus, Modestus, and Crescentia, Martyrs"
rank:
  roman-1960: commemoration
  divino-afflatu-1954: simplex
common:
  default: several-martyrs
texts:
  collect: collect.ss-vitus-modestus-crescentia
  lessons: lessons.ss-vitus-modestus-crescentia
sources:
  - citation: roman-martyrology
```

Required fields:

- `id`
- `kind`
- `title`
- at least one date rule or temporal generation rule
- at least one rank/status for a supported profile
- text references or common references sufficient to render applicable hours

Optional fields:

- `common`
- `proper_texts`
- `hour_extent`
- `commemoration_policy_hint`
- `transfer_policy_hint`
- `vigil`
- `octave`
- `source citations`

Observance data should not say "wins over X." That belongs to precedence rules.

## Temporal Data

Temporal observances are usually generated from date facts rather than listed
one by one.

Example:

```yaml
id: after-pentecost-weekday
kind: temporal_feria
generator:
  season: after_pentecost
  weekdays: [mon, tue, wed, thu, fri, sat]
title_pattern:
  la: "Feria {weekday} infra Hebdomadam {week} post Octavam Pentecostes"
  en: "{weekday} in the {week} week after Pentecost"
rank:
  roman-1960: fourth_class_feria
psalter_policy: ferial_psalter
collect_policy: current_sunday_collect
```

The generator creates a concrete candidate for a date. The candidate still goes
through occurrence like any other candidate.

## Calendar Data

Calendars list which observances are available on fixed or generated dates.

Calendar overlays are explicit:

```yaml
id: general-roman-1960
inherits:
  - general-roman-base
entries:
  "06-15":
    add:
      - ss-vitus-modestus-crescentia
```

Local calendars apply after the general calendar:

```yaml
id: local-urbis
inherits:
  - general-roman-1960
entries:
  "06-29":
    add:
      - saints-peter-and-paul-urbis-variant
```

The loader must flatten calendar inheritance for each supported profile and
report conflicts.

## Rank Model

Do not model rank as a float. The old project uses numeric values because Perl
strings and simple comparisons made that convenient, but a typed Rust model
should separate:

- rank label
- precedence bucket
- class
- office extent
- commemoration behavior
- profile-specific ordering

Conceptual model:

```text
Rank
  id: first_class_feast
  display_label: "I classis"
  precedence_group: Feast
  precedence_level: 700
  default_hour_extent: full_office
```

The `precedence_level` is only comparable inside the profile that defines it.

Each profile owns a rank table:

```yaml
rank_system:
  id: roman-1960
  ranks:
    first_class_feast:
      order: 700
      class: first
    second_class_feast:
      order: 600
      class: second
    third_class_feast:
      order: 500
      class: third
    fourth_class_feria:
      order: 100
      class: fourth
    commemoration:
      order: 50
      class: commemoration
```

Rules can inspect typed ranks and classes. Data should not rely on magic numbers.

## Text Model

Texts are structured blocks, not HTML strings.

Text roles include:

- antiphon
- psalm
- canticle
- chapter
- short_reading
- lesson
- responsory
- hymn
- versicle
- collect
- conclusion
- rubric
- heading
- blessing
- absolution

A text entry has:

```yaml
id: collect.ss-vitus-modestus-crescentia
role: collect
language: la
blocks:
  - type: prayer
    text: "..."
metadata:
  source: ...
```

Supported block types:

- `paragraph`
- `verse`
- `rubric`
- `versicle`
- `response`
- `antiphon`
- `psalm_verse`
- `heading`
- `prayer`
- `reference`

Text may reference other texts by ID, but references must be acyclic after
variant resolution. The loader must detect cycles before serving requests.

Text may contain limited inline markup:

- emphasis
- small caps
- red text semantic marker
- verse flex marker
- doxology marker
- alleluia marker

It may not contain raw HTML.

## Text Variants

Variants are allowed only over known dimensions.

Allowed condition dimensions:

- profile
- rite
- language
- season
- date range
- hour
- paschaltide
- lent
- advent
- old_hymns option
- psalter variant
- priest-led option

Example:

```yaml
id: hymn.lauds.after-pentecost
role: hymn
variants:
  - when:
      old_hymns: true
    text: hymn.aeterne-rerum-conditor.old
  - default:
      text: hymn.aeterne-rerum-conditor
```

Forbidden:

```yaml
when: "rank < 2 and not winner.rule.contains('No commemoratio')"
```

That must be Rust logic.

## Language and Translation

Calendar decisions are language-independent. Text resolution is language-aware.

Rules:

- Every office can be resolved with Latin alone.
- Additional languages are overlays on text IDs, not separate observances.
- Missing translations must be represented explicitly in the result.
- Fallback to Latin is allowed only when the request enables it or the renderer
  is a comparison/debug renderer.
- A text ID should refer to the same liturgical item across languages.

The resolved document should store parallel language columns as separate
localized blocks attached to the same semantic section when possible.

## Commons

Commons are reusable text sources. They are not observances unless requested as a
votive or special office.

Common references must state how they are used:

```yaml
common:
  default: several-martyrs
  profile_overrides:
    roman-1960: several-martyrs
```

Text resolution priority for a saint's missing text:

1. Proper text for the observance.
2. Proper text inherited from an observance variant.
3. Common text named by the observance.
4. Profile default common for the observance kind.
5. Error, unless the slot is optional.

The trace must record when a common supplies a text.

## Psalter Model

Psalter data must distinguish:

- psalm text
- canticle text
- antiphons
- psalmody pattern
- hour
- day of week
- seasonal variants
- profile variants

Conceptual schema:

```yaml
id: roman-weekly-1960
patterns:
  lauds:
    monday:
      variant: lauds-i
      items:
        - antiphon: ant.lauds.monday.1
          psalms: [psalm-46]
        - antiphon: ant.lauds.monday.2
          psalms: [psalm-5]
        - antiphon: ant.lauds.monday.3
          psalms: [psalm-28]
        - antiphon: ant.lauds.monday.4
          psalms: [canticle-monday-lauds]
        - antiphon: ant.lauds.monday.5
          psalms: [psalm-116]
```

The psalm resolver receives:

- profile
- hour
- day of week
- season
- principal observance
- common
- psalter policy

It returns a list of `PsalmodyItem`s with provenance.

## Ordinary Templates

An ordinary template defines the structure of an hour as semantic slots. It is
not an executable macro file.

Example for Lauds:

```yaml
id: ordinary.roman.lauds
hour: lauds
slots:
  - opening
  - psalmody
  - chapter
  - hymn
  - versicle
  - gospel_canticle:
      canticle: benedictus
      antiphon_role: benedictus_antiphon
  - preces:
      optional: true
  - collects
  - conclusion
  - marian_antiphon:
      optional: true
```

Slots may have simple profile conditions:

```yaml
- marian_antiphon:
    when_not:
      profile: roman-1960
```

Slots must not contain procedural text-resolution logic. Slot resolvers in Rust
own that logic.

## Runtime Data Catalog

After startup loading, the catalog should expose typed indexes:

```text
Catalog
  profiles_by_id
  calendars_by_id
  observances_by_id
  fixed_calendar_index
  temporal_generators
  transfer_index
  ordinary_templates
  psalter_schemes
  texts_by_id_and_language
  commons_by_id
  citations_by_id
```

The catalog is immutable after loading. Runtime services hold it in `Arc`.

The loader should optionally emit normalized debug JSON so humans can inspect the
resolved in-memory shape without reading internal Rust structs.

## Startup Loading And Validation

The startup loader performs these steps:

1. Read all source files.
2. Parse YAML with duplicate-key detection.
3. Validate schema.
4. Validate IDs.
5. Resolve profile inheritance.
6. Resolve calendar inheritance.
7. Resolve observance references.
8. Resolve text references.
9. Resolve common references.
10. Expand temporal generators into indexed definitions.
11. Validate rank availability for every profile.
12. Validate every required ordinary slot can be resolved for supported fixture
    dates.
13. Build the immutable in-memory catalog.
14. Optionally emit normalized debug JSON.
15. Optionally emit a validation report.

Validation errors must include:

- file path
- object ID
- field path
- human-readable message
- suggested fix where possible

Warnings are allowed for missing translations but not for missing Latin source
texts required by supported profiles.

The server must fail startup on validation errors in production. In development,
the CLI may expose a lenient mode that loads partial data for editor feedback,
but the resolver must mark incomplete results with diagnostics.

## Core Engine Modules

Recommended module responsibilities:

```text
date/
  Gregorian calendar, Easter, seasons, DateFacts.

catalog/
  Read-only access to normalized startup-loaded data.

calendar/
  Candidate providers for temporal, sanctoral, local, transfers, votives.

rubrics/
  Profile-specific occurrence, concurrence, commemoration, hour extent rules.

resolve/
  Converts ResolvedDay into HourPlan and OfficeDocument.

text/
  Text lookup, fallback, variants, references, localization.

render/
  Converts OfficeDocument into HTML/plain/JSON.

trace/
  Decision logging and explainability.

validation/
  Runtime assertions and debug checks.
```

The core engine must not depend on Dioxus, HTTP, CSS, or database libraries.

## Candidate Generation

Candidate generation creates possible observances before rubrical decisions.

Inputs:

- `DateFacts`
- `Profile`
- `CalendarScope`
- `Catalog`
- request options

Outputs:

```text
CandidateSet
  temporal: Vec<ObservanceCandidate>
  sanctoral: Vec<ObservanceCandidate>
  transferred: Vec<ObservanceCandidate>
  local: Vec<ObservanceCandidate>
  votive: Option<ObservanceCandidate>
```

Each `ObservanceCandidate` contains:

- observance ID
- candidate kind
- source calendar
- source date
- generated date
- profile rank
- title
- proper text references
- common references
- hour extent hints
- transfer state
- octave/vigil state
- provenance

Candidate generation must not choose a winner.

## Candidate Adjustment

Candidate adjustment handles named profile-specific corrections before
occurrence.

Examples:

- Suppress ordinary vigils on Sundays except where the profile permits them.
- Remove octaves abolished in the selected profile.
- Add or remove local-calendar candidates.
- Convert a pre-1960 semiduplex to a lower status in a reduced profile.
- Apply permanent transfers.
- Apply annual transfers based on Easter.
- Mark a candidate as "commemoration only."

Each adjustment rule must have:

- stable name
- phase
- profile
- predicate
- action
- source citation or implementation comment
- unit tests

The trace must record every adjustment that changes the candidate set.

## Occurrence

Occurrence chooses the principal office for non-vesperal hours.

Inputs:

- `AdjustedCandidateSet`
- `DateFacts`
- `Profile`
- `Hour`

Output:

```text
OccurrenceDecision
  principal: ObservanceCandidate
  displaced: Vec<DisplacedCandidate>
  reasons: Vec<TraceEvent>
```

The occurrence resolver must:

1. Filter candidates not active for the requested hour.
2. Compare temporal and sanctoral candidates using the profile's precedence
   comparator.
3. Apply explicit exceptions such as feasts of the Lord and privileged Sundays.
4. Preserve displaced candidates for possible commemoration or transfer.
5. Return a single principal candidate or a typed ambiguity error.

The comparator must be profile-specific. There is no universal rank ordering.

## Concurrence

Vespers and sometimes Compline require concurrence: today's second Vespers may
compete with tomorrow's first Vespers.

The concurrence resolver must:

1. Resolve today's candidate set for second Vespers eligibility.
2. Resolve tomorrow's candidate set for first Vespers eligibility.
3. Compare the two according to the profile's concurrence rules.
4. Select principal Vespers.
5. Determine commemorations from the losing side where allowed.

Concurrence must not reuse a mutated global "tomorrow" state. It should call the
same pure candidate generation and adjustment pipeline for both dates and then
compare the two values.

## Commemorations

Commemoration resolution is its own phase.

Inputs:

- principal candidate
- displaced candidates
- candidate set
- date facts
- hour
- profile

Output:

```text
CommemorationSet
  items: Vec<Commemoration>
```

A `Commemoration` contains:

- observance ID
- title
- scope, such as LaudsOnly, LaudsAndVespers, MassOnly, MatinsAndLauds
- text source references
- rank/status
- reason

The commemoration resolver decides whether the commemoration exists at the
requested hour. The collect renderer must not decide this.

Common suppression cases belong here:

- no commemoration under a higher feast
- only at Lauds under 1960
- vigil only at Mass
- octave commemoration omitted
- Sunday commemoration suppressed by a feast of the Lord, if the profile says so

## ResolvedDay

`ResolvedDay` is the first durable result of liturgical decision-making.

It contains:

```text
ResolvedDay
  request
  date_facts
  profile_id
  principal
  rank
  season
  temporal_context
  commemorations
  psalter_policy
  collect_policy
  hymn_policy
  text_fallback_context
  trace
```

After this point, no code should revisit occurrence or concurrence. Later phases
may ask "what is the principal?" but not "should the saint have won?"

## HourPlan

`HourPlan` applies the ordinary template to the resolved day.

Inputs:

- `ResolvedDay`
- ordinary template for the hour
- profile
- options

Output:

```text
HourPlan
  hour
  title
  slots: Vec<PlannedSlot>
```

Each `PlannedSlot` says:

- slot kind
- required or optional
- resolver to use
- context overrides
- trace parent

For Lauds, the planned slots normally are:

1. Opening
2. Psalmody
3. Chapter
4. Hymn
5. Versicle
6. Benedictus with antiphon
7. Preces, if applicable
8. Main collect
9. Commemoration collects
10. Conclusion
11. Optional Marian antiphon, depending on profile

## Text Resolution

Text resolution fills each planned slot.

Inputs:

- `PlannedSlot`
- `ResolvedDay`
- `Catalog`
- language
- options

Output:

```text
ResolvedSection
  slot_kind
  localized_blocks
  provenance
```

General fallback priority:

1. Proper text of the principal observance.
2. Profile-specific proper variant.
3. Common referenced by the principal observance.
4. Temporal or seasonal text.
5. Psalter text.
6. Ordinary text.
7. Explicit profile fallback.
8. Error if required; empty if optional.

The fallback chain differs by slot. Each slot resolver must document its exact
priority.

Text resolution may choose among available texts. It may not decide occurrence,
commemoration existence, rank, or transfer.

## Collect Resolution

Collects need special structure because multiple prayers may share one
conclusion.

Do not model this by trimming strings.

Use:

```text
CollectGroup
  main: CollectItem
  commemorations: Vec<CollectItem>
  conclusion_policy: ConclusionPolicy
```

Conclusion policies:

- `EachPrayerOwnConclusion`
- `SharedConclusion`
- `MainConclusionOnly`
- `NoConclusion`

The profile and hour decide the policy. Text data stores the prayer body and
conclusion separately where possible.

For a feria whose policy is `current_sunday_collect`, the collect resolver
directly resolves the current Sunday's collect from the temporal context.

## Psalmody Resolution

Psalmody resolution must return structured items:

```text
PsalmodyItem
  antiphon: ResolvedText
  psalms: Vec<ResolvedPsalmOrCanticle>
  repetition_policy: AntiphonRepetitionPolicy
  provenance
```

Antiphon repetition is a profile/rank/hour decision. It must not be hard-coded in
HTML rendering.

Psalmody source priority:

1. Proper psalmody of principal observance.
2. Proper antiphons with psalms from common or psalter, if profile permits.
3. Common psalmody.
4. Seasonal psalmody.
5. Ferial psalter pattern.

The exact chain is profile-specific and should be implemented by a slot resolver
selected by the profile.

## Gospel Canticles

Lauds uses the Benedictus. Vespers uses the Magnificat. Compline may use the
Nunc Dimittis depending on profile and rite.

The canticle text itself is stable data. The antiphon is resolved separately.

Benedictus antiphon priority:

1. Proper Benedictus antiphon of the principal.
2. Common Benedictus antiphon.
3. Seasonal/temporal antiphon.
4. Ferial psalter antiphon.
5. Error if required.

Commemoration antiphons use the commemorated observance's proper/common chain,
not the principal's chain.

## Preces

Preces are not just text. Their presence depends on:

- hour
- weekday
- season
- rank
- profile
- whether the office is ferial
- whether kneeling or penitential rubrics apply
- whether the request has priest-led mode

The preces resolver returns either:

- no section, with a trace reason
- a structured `PrecesSection`

The renderer does not decide whether preces are present.

## Alleluia and Seasonal Text Mutation

Alleluia suppression, addition, or doubling must be represented as text
transforms with explicit contexts.

Do not mutate raw strings in random slot resolvers.

Use a text transform phase:

```text
ResolvedText
  -> TextTransformContext
  -> ResolvedText
```

Transforms include:

- suppress alleluia outside Paschaltide where required
- add alleluia in Paschaltide where required
- choose Paschaltide responsory variant
- choose Lent or Advent variant

Transforms must record trace events.

## Rendering

Renderers receive only `OfficeDocument`.

They may:

- choose layout
- choose typography
- hide or show rubrics
- produce columns
- show source traces
- add links and anchors

They may not:

- choose the winner
- add commemorations
- suppress commemorations
- change psalms
- choose a collect source
- apply rank rules

`OfficeDocument` should be serializable so the UI can render from JSON if needed.

## OfficeDocument

The final domain result should look like:

```text
OfficeDocument
  metadata:
    request
    title
    rank
    date
    profile
    calendar_scope
  sections:
    Vec<OfficeSection>
  commemorations:
    Vec<CommemorationSummary>
  trace:
    Vec<TraceEvent>
  diagnostics:
    Vec<Diagnostic>
```

Each `OfficeSection` has:

- section ID
- kind
- heading
- blocks by language
- provenance
- optional rubrics

Blocks are semantic. HTML is produced later.

## Trace and Explainability

Every non-trivial decision should be traceable.

Trace events should include:

- phase
- rule name
- input summary
- output summary
- affected IDs
- source citation if available
- debug severity

Example trace for June 15, 2026:

```text
date: computed after-pentecost week context
candidate: generated temporal feria after-pentecost-week-3-monday
candidate: found sanctoral ss-vitus-modestus-crescentia on 06-15
occurrence: roman-1960 low-rank saint does not displace fourth-class feria
commemoration: saint retained at Lauds only
collect: principal uses current Sunday collect by temporal collect policy
psalmody: selected Monday Lauds I from roman-weekly-1960 psalter
```

The UI should eventually expose this as "Why this office?".

## Error Handling

Runtime APIs must return typed errors.

Error kinds:

- invalid request
- unsupported profile
- unsupported hour
- missing calendar
- missing observance
- missing required text
- ambiguous occurrence
- invalid catalog
- renderer error

Errors must include enough context for a data editor:

```text
Missing required text:
  text role: benedictus_antiphon
  observance: ss-example
  profile: roman-1960
  language: la
  tried:
    proper.ss-example.benedictus_antiphon
    common.several-martyrs.benedictus_antiphon
    psalter.lauds.monday.benedictus_antiphon
```

The core must not panic for bad data or bad requests.

## Caching

The engine is pure, so caching is straightforward.

Cache keys:

```text
date
hour
profile_id
calendar_scope
languages
options relevant to text/rubrics
catalog_version
```

Cache values:

- `ResolvedDay` for date/profile/calendar/options
- `OfficeDocument` for date/hour/profile/languages/options
- rendered HTML for exact UI options

Cache invalidation is by catalog version.

## Concurrency

The catalog is immutable and shared by reference.

All resolution functions should be thread-safe. No global mutable state is
allowed. Request-specific state lives in local values passed through the
pipeline.

This allows:

- parallel snapshot tests
- concurrent HTTP requests
- background precomputation
- comparison across profiles

## Server API

Recommended HTTP endpoints:

```text
GET /api/office
  date=2026-06-15
  hour=lauds
  profile=roman-1960
  calendar=general
  languages=la,en
  format=json

GET /api/office/html
  same query, returns rendered HTML fragment

GET /api/explain
  same query, returns trace-focused JSON

GET /api/catalog/profiles
GET /api/catalog/calendars
GET /api/catalog/languages
```

The server normalizes query strings into `OfficeRequest`, calls the core, caches
results, and returns serialized output.

## UI

The Dioxus UI should be a client of the API/core, not a place where liturgical
logic lives.

Primary UI features:

- date picker
- hour selector
- profile selector
- calendar selector
- language columns
- display options
- office output
- optional explanation panel
- source/provenance hover or panel

The UI should consume `OfficeDocument` and render semantic blocks. It should not
assemble offices.

## Importing Divinum Officium Data

The importer is a migration tool only.

It may:

- parse original sectioned files
- infer observance IDs
- infer text IDs
- convert commons and proper references
- produce draft YAML
- mark uncertain mappings
- generate comparison fixtures

It must not:

- make the runtime read original files
- preserve implicit Perl globals
- preserve executable marker expansion as a runtime model

Every imported record should include provenance:

```yaml
imports:
  divinum_officium:
    path: web/www/horas/Latin/Sancti/06-15.txt
    section: Oratio
    imported_at: 2026-06-15
    confidence: manual_review_required
```

## Comparison Against Divinum Officium

During migration, Divinum Officium should be used as an oracle, not as the target
architecture.

Comparison tests should:

- run known dates through Divinum Officium
- run the same requests through the Rust engine
- compare structural sections first
- compare normalized text second
- ignore expected typography differences
- record intentional divergences

The comparison harness should store:

- request
- original output hash
- Rust output hash
- text diff
- structural diff
- trace
- status: match, accepted-difference, failure

## Testing Strategy

Unit tests:

- Gregorian date math.
- Easter calculation.
- season calculation.
- temporal key generation.
- rank comparison per profile.
- occurrence edge cases.
- concurrence edge cases.
- commemoration suppression.
- collect policy.
- psalter selection.
- text fallback.
- language fallback.
- text transforms.

Data validation tests:

- all IDs unique
- all references resolve
- no text reference cycles
- every supported profile has required ranks
- every ordinary slot has a resolver
- every required slot resolves for fixture dates
- every translation references an existing source text ID

Golden tests:

- fixed list of dates across the year
- all hours for those dates
- major feasts
- ferias
- vigils
- octaves
- Sundays
- transferred feasts
- local calendar examples
- profile comparison examples

Property tests:

- every date in a supported range resolves at least title, rank, and hour
  skeleton
- no required section is silently empty
- no two principal observances are returned
- trace contains an occurrence decision
- renderer output is stable for the same document

## Required Fixtures

Minimum initial fixtures for Roman 1960:

- 2026-06-15 Lauds: Monday after Pentecost with Ss. Vitus, Modestus, and
  Crescentia as Lauds-only commemoration.
- Christmas Day.
- Epiphany.
- Ash Wednesday.
- Palm Sunday.
- Maundy Thursday.
- Good Friday.
- Holy Saturday.
- Easter Sunday.
- Monday in Easter octave.
- Ascension.
- Pentecost.
- Trinity Sunday.
- Corpus Christi.
- Sacred Heart.
- Immaculate Conception on a Sunday case.
- All Saints and All Souls adjacency.
- A date with no sanctoral observance.
- A date with a higher sanctoral feast.
- First Vespers vs second Vespers concurrence.

## Example: Lauds on 2026-06-15

Request:

```text
date: 2026-06-15
hour: lauds
profile: roman-1960
calendar: general
languages: [la, en]
```

Date facts:

```text
weekday: monday
fixed date: 06-15
season: after_pentecost
temporal context: monday of week 3 after Pentecost, per profile terminology
```

Generated candidates:

```text
temporal:
  after-pentecost-week-3-monday
  rank: fourth_class_feria
  collect_policy: current_sunday_collect
  psalter_policy: ferial_psalter

sanctoral:
  ss-vitus-modestus-crescentia
  rank/status under roman-1960: commemoration or equivalent low rank
  common: several-martyrs
```

Occurrence:

```text
principal: after-pentecost-week-3-monday
displaced: ss-vitus-modestus-crescentia
reason: roman-1960 low-rank saint does not displace this feria
```

Commemoration:

```text
items:
  - ss-vitus-modestus-crescentia
    scope: lauds_only
    text chain: proper, then several-martyrs common
```

Hour plan:

```text
ordinary: roman lauds
slots:
  opening
  psalmody
  chapter
  hymn
  versicle
  benedictus
  preces if applicable
  main collect
  commemoration collect
  conclusion
```

Psalmody:

```text
source: roman weekly psalter
day: monday
hour: lauds
variant: lauds-i
```

Collect:

```text
main collect source: current Sunday collect
reason: principal temporal feria has collect_policy current_sunday_collect
```

Rendering receives a finished `OfficeDocument`; it does not know why the saint is
only commemorated.

## Exception Handling Strategy

Exceptions are inevitable. The architecture should make them visible, local, and
testable.

Use three mechanisms:

1. Typed Rust rules for real rubrical logic.
2. Declarative data for facts and simple variants.
3. Named adjustment rules for rare profile-specific cases.

Avoid:

- one giant generic rules engine
- hidden conditionals in renderers
- arbitrary expressions inside YAML
- mutable global "current office" state

Rule placement:

```text
Date exception            -> date/profile module
Candidate addition/removal -> candidate adjustment
Winner decision            -> occurrence/concurrence
Commemoration existence    -> commemoration resolver
Hour applicability         -> hour extent resolver
Text source choice         -> text resolver
Typography/display         -> renderer
```

If a new exception is discovered, first ask which phase owns the concept. Put the
condition there and add a trace event and test.

## Resolution Rules

This section defines the actual resolver behavior. It is not a list of named
rubrical facts; it is the algorithm an implementation follows to turn a request
and the YAML-backed catalog into an `OfficeDocument`.

All functions below return both their value and trace events. When a required
lookup fails, the function returns a typed error. When an optional lookup fails,
the function returns `None` with an omission trace.

### Top-Level Office Resolution

`resolve_office(request, catalog)` runs exactly these phases:

1. Validate that `request.profile_id`, `request.hour`, `request.calendar_scope`,
   and `request.languages` exist in the loaded catalog.
2. Load the `Profile` by ID.
3. Compute `DateFacts` from the Gregorian date and profile.
4. If `request.hour` is Vespers or a profile-defined concurrent hour, call
   `resolve_concurrent_day`; otherwise call `resolve_occurring_day`.
5. Build an `HourPlan` from the resolved day and the ordinary template.
6. Resolve each planned slot into semantic sections.
7. Resolve each selected text ID into the requested languages.
8. Apply text transforms such as alleluia handling and seasonal variants.
9. Return an `OfficeDocument`.

No later phase may redo an earlier decision. For example, once
`resolve_occurring_day` chooses a principal, slot resolution may ask what the
principal is, but it may not decide whether another candidate should have won.

### Date Fact Resolution

`compute_date_facts(date, profile)` produces the date-only facts used by later
phases.

1. Reject dates before 1582-10-15 unless the profile explicitly supports them.
2. Compute day of week with Sunday as `0`, Monday as `1`, and Saturday as `6`.
3. Compute Gregorian Easter for the year.
4. Compute the first Sunday of Advent as the Sunday from November 27 through
   December 3 inclusive.
5. Compute day-of-year numbers for the date, Easter, Advent I, Christmas, and
   January 6.
6. Compute the temporal context using the selected profile's temporal calendar.

The initial Roman temporal context uses this order:

1. If the date is from Advent I through December 24, return Advent week/day.
2. If the date is December 25 or later, return Nativity/Christmas context.
3. If the date is early January before the profile's post-Epiphany transition,
   return Nativity/Epiphany context.
4. If the date is before Septuagesima, return after-Epiphany context.
5. If the date is in Septuagesima, Sexagesima, or Quinquagesima, return
   pre-Lent context.
6. If the date is before Easter, return Lent, Passiontide, or Holy Week context.
7. If the date is Easter through the profile's Pentecost boundary, return
   Paschal context.
8. Otherwise return after-Pentecost context, including any late-year displaced
   after-Epiphany weeks required by the profile.

The date layer also marks ember days, rogation days, octaves, vigils, and
special temporal days as facts. It does not decide which observance wins.

### Catalog Lookup Rules

The startup loader builds the catalog indexes used by the resolver. Runtime code
does not walk arbitrary YAML nodes.

When resolving an object by ID:

1. Look up the ID in the typed index for that object kind.
2. Apply profile-specific field overlays.
3. Apply calendar/local overlays only where the request's calendar scope permits
   them.
4. Return the typed object plus provenance.

If multiple overlays affect the same field, use this order:

1. base object;
2. inherited profile overlay;
3. selected profile overlay;
4. general calendar overlay;
5. local calendar overlays in request order;
6. explicit request option overlay.

The loader must reject ambiguous overlays that try to set the same scalar field
at the same priority.

### Candidate Generation

`generate_candidates(date_facts, profile, calendar_scope, catalog)` creates a
`CandidateSet`. It does not choose the principal.

Temporal candidates:

1. Select the most specific temporal generator matching `DateFacts`.
2. Generate the normal temporal candidate for the day.
3. Generate additional temporal candidates only when the profile has explicit
   data for a vigil, octave, transferred day, or other secondary temporal
   observance.
4. Attach temporal policies such as `psalter_policy`, `collect_policy`,
   `hymn_policy`, and `ordinary_policy`.

Temporal specificity order is:

1. Triduum and named privileged days.
2. Named movable feasts.
3. Octave days.
4. Named Sundays.
5. Named ferias, such as ember days or rogation days.
6. Seasonal weekdays.
7. Ordinary ferias.

Sanctoral candidates:

1. Convert the date to a fixed-date key, such as `06-15`.
2. Read the merged calendar entries for that key.
3. For each observance ID, load the observance and its profile status.
4. Add each observance as a sanctoral candidate.

Transfer candidates:

1. Read permanent transfers keyed by fixed date.
2. Read annual transfers keyed by Easter-dependent transfer tables.
3. Read temporal transfers keyed by temporal context.
4. For each transfer, create a candidate with both original and observed date.

Votive candidates:

1. If no votive is requested, create none.
2. If a votive is requested, create the votive candidate and preserve normal
   candidates for possible commemorations.
3. The votive does not automatically win. The selected profile must permit it.

Every candidate must contain:

- observance ID;
- candidate kind;
- source calendar or generator;
- original date and observed date;
- profile rank/status;
- eligibility as principal;
- active hours;
- text sources;
- common references;
- flags such as feast of the Lord, vigil, octave, feria, Sunday, or saint.

### Candidate Normalization

`normalize_candidates(candidate_set, profile, date_facts)` applies profile
statuses before occurrence.

For each candidate:

1. If the profile has no rank/status for the candidate, remove it and trace the
   removal.
2. If the profile abolishes the candidate, remove it and trace the removal.
3. If the profile reduces the candidate to commemoration-only, set
   `principal_eligible = false` and keep it as a possible commemoration.
4. If the profile marks the candidate Mass-only, remove it from Office
   resolution but keep it visible in diagnostics.
5. Compute active hours from the profile's hour-extent rules.
6. Compute profile precedence keys.

For Roman 1960, a sanctoral observance whose status is only a commemoration is
not principal-eligible. This is why June 15 can produce a saint commemoration
without a saint office.

### Occurring Day Resolution

`resolve_occurring_day(request, date_facts, candidate_set)` resolves all
non-concurrent hours.

1. Normalize candidates.
2. Select the best temporal candidate:
   - choose the temporal candidate with the highest temporal specificity;
   - if two temporal candidates have equal specificity, choose the higher
     profile precedence;
   - if still tied, fail with `AmbiguousTemporalCandidate`.
3. Select the best sanctoral candidate:
   - keep only sanctoral candidates active for the requested hour;
   - keep only candidates with `principal_eligible = true`;
   - choose the highest profile precedence;
   - if tied, use calendar priority and stable ID as deterministic tie-breakers
     and emit a warning trace.
4. If there is no best sanctoral candidate, the temporal candidate is principal.
5. If there is a sanctoral candidate, compare it with the temporal candidate
   using the profile's occurrence comparator.
6. The comparator returns `TemporalWins`, `SanctoralWins`, or
   `UnsupportedOccurrenceCase`.
7. The losing candidate and all unused candidates become displaced candidates.
8. Resolve commemorations from the displaced candidates.
9. Return `ResolvedDay`.

The Roman 1960 occurrence comparator uses this order:

1. A non-principal-eligible candidate cannot win.
2. A first-class sanctoral candidate beats a lower temporal candidate.
3. A second-class sanctoral feast of the Lord may beat a second-class or lower
   Sunday where the profile permits it.
4. Other Sundays are preferred to sanctoral candidates of equal or lower
   precedence.
5. Otherwise the higher profile precedence wins.
6. If precedence is equal, temporal wins unless the profile has a named
   exception.

### Concurrent Day Resolution

`resolve_concurrent_day(request, date_facts, catalog)` resolves Vespers and any
other profile-defined concurrent hour.

1. Resolve today's occurring day as if for the corresponding daytime context.
2. Resolve tomorrow's occurring day using tomorrow's `DateFacts`.
3. Convert today's principal into a second-Vespers candidate if its active hours
   include second Vespers.
4. Convert tomorrow's principal into a first-Vespers candidate if its active
   hours include first Vespers.
5. If only one side has an active Vespers candidate, that side is principal.
6. If both sides are active, compare them with the profile's concurrence
   comparator.
7. Pass the losing side to commemoration resolution.
8. Return a `ResolvedDay` whose principal is the Vespers winner and whose
   temporal context still records the requested civil date.

For Roman 1960:

- first-class days normally have first Vespers;
- Sundays normally have first Vespers;
- most second-class feasts do not have first Vespers;
- second-class feasts of the Lord have first Vespers when the profile permits;
- equal-rank concurrence prefers the preceding office unless a named exception
  applies.

### Commemoration Resolution

`resolve_commemorations(principal, displaced, request, profile)` returns a
`CommemorationSet`.

1. Start with displaced candidates from occurrence or concurrence.
2. Add explicit commemorations attached to the principal observance.
3. Add required temporal commemorations attached to the date facts.
4. Remove candidates whose commemoration status is suppressed by the profile.
5. Remove candidates blocked by the principal's `no_commemorations` policy.
6. For each remaining candidate, compute commemoration scope:
   - full major-hour commemoration;
   - Lauds-only;
   - Vespers-only;
   - Matins-and-Lauds;
   - Mass-only;
   - omitted.
7. Keep only commemorations whose scope includes the requested hour.
8. Sort commemorations by profile commemoration order.
9. Return the result with trace events for kept and omitted candidates.

For Roman 1960:

- low sanctoral observances on temporal ferias may become Lauds-only
  commemorations;
- very low sanctoral observances are omitted;
- a feast of the Lord can suppress another feast-of-the-Lord commemoration where
  the profile requires it;
- a commemoration-only saint does not become the principal office merely because
  the temporal day is a feria.

### ResolvedDay Construction

`ResolvedDay` is built after occurrence or concurrence and commemoration.

It contains:

- request;
- date facts;
- principal candidate;
- displaced candidates;
- commemorations;
- temporal context;
- rank/status for display;
- psalter policy;
- collect policy;
- hymn policy;
- text fallback context;
- trace.

After `ResolvedDay` exists, the system must not revisit principal selection.

### Hour Plan Resolution

`build_hour_plan(resolved_day, profile, hour)` chooses the ordinary template and
turns it into planned slots.

1. Load the ordinary template named by the profile and hour.
2. Evaluate slot conditions against profile, hour, date facts, principal,
   commemoration set, and request options.
3. Keep slots whose conditions pass.
4. Omit optional slots whose conditions fail and trace the omission.
5. Fail if a required slot has no registered resolver.

Roman Lauds normally plans:

1. opening;
2. psalmody;
3. chapter;
4. hymn;
5. versicle;
6. Benedictus with antiphon;
7. preces if applicable;
8. collects;
9. conclusion.

Under Roman 1960, the ordinary suffrage and the final Marian antiphon are not
part of the Lauds hour body.

### Generic Text Role Resolution

`resolve_text_role(owner, role, context)` resolves a text role for a principal,
commemoration, common, temporal context, psalter, or ordinary.

The lookup order is:

1. exact proper text for `owner + role + hour`;
2. exact proper text for `owner + role`;
3. profile-specific proper variant;
4. common text for `owner.common + role + hour`;
5. common text for `owner.common + role`;
6. temporal or seasonal text for `context.temporal + role + hour`;
7. temporal or seasonal text for `context.temporal + role`;
8. psalter text for `profile.psalter + role + hour + weekday`;
9. ordinary text for `profile.ordinary + role + hour`;
10. optional omission or required-text error.

Not every slot uses every step. Slot-specific rules below override this generic
order where necessary.

### Psalmody Resolution

`resolve_psalmody(resolved_day, hour)` returns a list of `PsalmodyItem`.

1. If the principal supplies a complete proper psalmody pattern for the hour,
   use it.
2. Else if the principal's common supplies a complete psalmody pattern, use it.
3. Else select a psalter pattern from the profile psalter.
4. For Lauds, first compute the Lauds variant:
   - use explicit principal `lauds_variant` if present;
   - else use Lauds II when the principal is temporal and the date is an Advent
     weekday, Lent day, or non-Paschaltide ember day;
   - otherwise use Lauds I.
5. Select the psalter pattern by hour, weekday, and variant.
6. Resolve antiphons:
   - complete proper antiphon set of the principal;
   - complete antiphon set from the common;
   - seasonal antiphon set;
   - antiphons already attached to the psalter pattern.
7. If an antiphon set is supplied without psalms, pair it by index with the
   selected psalter psalms.
8. Resolve psalm and canticle text IDs.
9. Compute antiphon repetition policy from profile, rank, season, and hour.
10. Return structured `PsalmodyItem`s.

For 2026-06-15 Lauds under Roman 1960, the principal is a temporal after-
Pentecost feria, no proper psalmody overrides the slot, and Lauds I is selected.
The psalter pattern is Monday Lauds I.

### Chapter Resolution

`resolve_chapter(resolved_day, hour)` uses:

1. principal proper chapter for the hour;
2. principal proper chapter without hour qualifier;
3. common chapter for the principal's common;
4. temporal/seasonal chapter for the current temporal context;
5. ordinary or psalter fallback;
6. required-text error.

The commemoration's chapter is not used for the main chapter.

### Hymn Resolution

`resolve_hymn(resolved_day, hour)` uses:

1. principal proper hymn for the hour;
2. common hymn for the principal's common;
3. seasonal hymn selected by profile, season, and hour;
4. ordinary hymn for the hour;
5. required-text error.

If the request has an old-hymns option and the profile supports it, the selected
hymn text ID is passed through typed variant selection before language lookup.

### Versicle Resolution

`resolve_versicle(resolved_day, hour)` uses:

1. principal proper versicle for the hour;
2. common versicle for the principal's common;
3. temporal/seasonal versicle;
4. ordinary versicle;
5. required-text error.

The versicle used in a commemoration is resolved separately from the main
versicle and uses the commemorated observance as owner.

### Gospel Canticle Resolution

`resolve_gospel_canticle(resolved_day, hour)` chooses the canticle by hour:

- Lauds uses Benedictus;
- Vespers uses Magnificat;
- Compline uses Nunc Dimittis where the profile includes it.

The canticle text is fixed by the ordinary/profile. Its antiphon is resolved in
this order:

1. principal proper gospel-canticle antiphon for the hour;
2. common gospel-canticle antiphon for the principal's common;
3. temporal/seasonal gospel-canticle antiphon;
4. psalter antiphon for the hour and weekday;
5. required-text error.

Commemoration antiphons are not substituted for the main gospel-canticle
antiphon. They are resolved in the collect/commemoration section.

### Preces Resolution

`resolve_preces(resolved_day, hour)` returns an optional section.

1. If the ordinary template has no preces slot for this hour, return `None`.
2. If the principal or profile says to omit preces, return `None`.
3. If the rank/status is too solemn for preces under the profile, return `None`.
4. If the temporal context suppresses preces, return `None`.
5. Determine whether ferial or dominical preces are requested:
   - ferial preces are considered on non-sanctoral ferial days when the temporal
     context is Advent, Lent, an ember day, or explicitly marked for preces;
   - dominical preces are considered only where the profile and ordinary request
     them.
6. Under Roman 1960, ferial preces are permitted only on Wednesday, Friday, and
   ember days, after the previous conditions pass.
7. If a high commemoration or octave blocks dominical preces under the profile,
   return `None`.
8. Resolve the preces text from the ordinary/special psalter source for the hour.

### Main Collect Resolution

`resolve_main_collect(resolved_day, hour)` returns a `CollectItem`.

1. If the principal has `collect_policy = current_sunday_collect`, resolve the
   Sunday temporal observance for the current temporal week and use its collect.
2. Else use principal proper collect for the hour.
3. Else use principal proper collect without hour qualifier.
4. Else use common collect for the principal's common.
5. Else, if the principal is a temporal feria and the profile says ferias use
   the current Sunday collect, resolve the current Sunday collect.
6. Else return `MissingRequiredText`.

The current Sunday collect is found by semantic temporal context, not by
constructing a file path. For example, a Monday after Pentecost points to the
Sunday of that after-Pentecost week.

### Commemoration Text Resolution

`resolve_commemoration_text(commemoration, hour)` returns the antiphon, versicle,
and collect for one commemoration.

For the antiphon:

1. commemorated observance proper antiphon for the hour or commemoration slot;
2. commemorated observance common antiphon;
3. profile-defined commemoration fallback;
4. required-text error.

For the versicle:

1. commemorated observance proper versicle;
2. commemorated observance common versicle;
3. profile-defined commemoration fallback;
4. required-text error.

For the collect:

1. commemorated observance proper collect;
2. commemorated observance hour-specific collect;
3. commemorated observance common collect;
4. required-text error.

The principal's common is never used for a commemoration unless the
commemorated observance itself references the same common.

### Collect Group Resolution

`resolve_collects(resolved_day, hour)` runs:

1. Resolve the main collect.
2. Resolve commemoration text for every commemoration active at this hour.
3. Build a `CollectGroup` with one main collect and zero or more commemorations.
4. Choose conclusion policy from the profile and hour:
   - each prayer has its own conclusion;
   - shared conclusion;
   - main conclusion only;
   - no conclusion.
5. Attach conclusion text structurally.

The implementation must not remove conclusions by regex or string trimming.

### Language Resolution

`localize_text(text_id, language, context)` resolves the chosen text into a
language.

1. Load the canonical text record by ID.
2. Select the variant whose typed conditions best match the context.
3. Look for that text ID in the requested language.
4. If present, return it.
5. If absent and language fallback is enabled, return the Latin text with a
   missing-translation diagnostic.
6. If absent and strict translation is enabled, return `MissingTranslation`.

Variant matching order:

1. exact profile;
2. exact rite;
3. exact hour;
4. exact season or temporal context;
5. exact option match, such as old hymns;
6. default variant.

If two variants match at the same specificity, startup validation must reject the
data as ambiguous.

### Text Transform Resolution

`apply_text_transforms(resolved_text, context)` runs after source and language
selection.

Transforms include:

- suppressing alleluia where the profile requires it;
- adding alleluia in Paschaltide where the profile requires it;
- choosing doxology variants;
- applying flex markers;
- replacing name placeholders in common prayers;
- applying semantic red/rubric markers.

Transforms operate on structured text blocks, not HTML strings.

### Final Document Assembly

`assemble_document(resolved_day, hour_plan, sections)` creates the
`OfficeDocument`.

It must include:

- request metadata;
- date facts;
- principal title and rank;
- commemoration summary;
- ordered sections;
- diagnostics;
- trace.

The renderer receives this document and only formats it.

### Worked Resolution: 2026-06-15 Lauds, Roman 1960

1. Request is `date=2026-06-15`, `hour=Lauds`, `profile=roman-1960`.
2. Date facts compute Monday after Pentecost context.
3. Candidate generation creates:
   - temporal candidate: Monday feria after Pentecost, with
     `collect_policy = current_sunday_collect`;
   - sanctoral candidate: Ss. Vitus, Modestus, and Crescentia.
4. Candidate normalization marks the saint as not principal-eligible under
   Roman 1960 but possible as a commemoration.
5. Occurrence chooses the temporal feria as principal.
6. Commemoration resolution keeps the saint as a Lauds-only commemoration.
7. The Lauds ordinary creates slots for opening, psalmody, chapter, hymn,
   versicle, Benedictus, collects, and conclusion. It omits suffrage and the
   final Marian antiphon inside the hour for Roman 1960.
8. Psalmody resolution selects Roman weekly psalter, Monday, Lauds I.
9. Main collect resolution follows `current_sunday_collect` and uses the Sunday
   collect of the current after-Pentecost week.
10. Commemoration text resolution uses the saint's proper texts, then its martyr
    common for missing antiphon/versicle/collect pieces.
11. Language resolution produces Latin and English columns from the same
    semantic text IDs.
12. The renderer formats the finished `OfficeDocument`.

## Resolver Function Contract

Every resolver function must:

- accept typed input
- return typed output, an optional omission, or a typed error
- not read global state
- not write global state
- record trace events for decisions, fallbacks, and omissions
- be covered by at least one fixture or unit test

Resolver names should be stable because traces and tests refer to them. Examples:

```text
resolve_occurring_day
resolve_concurrent_day
resolve_commemorations
resolve_psalmody
resolve_main_collect
resolve_commemoration_text
```

Profile-specific exception handling may still exist inside these functions, but
the public trace should identify which resolver made the decision and what input
caused it.

## Data Validation Contract

The startup loader/validator must reject:

- duplicate IDs
- duplicate YAML keys
- unknown fields
- unsupported condition dimensions
- unresolved text references
- unresolved common references
- unsupported rank names
- unknown profile IDs
- cyclic text references
- observances without any active rank in supported profiles
- ordinary templates with unknown slot kinds
- required slots with no resolver

The startup loader/validator may warn:

- missing non-Latin translation
- imported record requiring manual review
- source citation missing for draft data
- text available but unused

Warnings must not block local experimentation but should block release builds
unless explicitly allowed.

## Determinism

All maps must be sorted before serialization. All candidate comparisons must have
deterministic tie-breakers. All traces must be emitted in deterministic order.

Tie-breakers should be explicit:

1. precedence rule result
2. rank order
3. observance kind order defined by profile
4. calendar priority
5. stable observance ID

If a tie reaches the final ID fallback, emit a warning trace. That usually means
the data or profile comparator needs a better rule.

## Security

Source data is not trusted executable input.

Requirements:

- no eval
- no arbitrary scripting in data
- no raw HTML from source texts
- renderer escapes all text
- importer sanitizes legacy markup
- API rejects unknown enum values
- optional prebuilt bundle loader validates catalog version and checksum

## Performance Targets

Initial targets:

- catalog load under 200 ms in release mode
- single office resolution under 10 ms after catalog load
- rendered HTML under 20 ms for normal two-column output
- full-year validation under 5 seconds for one profile after catalog load

Correctness has priority over these targets, but the architecture should avoid
known slow paths such as reparsing and revalidating YAML on each request.

## Development Workflow

Recommended workflow:

1. Edit source YAML.
2. Run the loader/validator through the CLI or dev server startup.
3. Fix validation errors.
4. Run focused unit tests.
5. Run fixture date snapshots.
6. Run Divinum Officium comparison tests for affected profiles.
7. Inspect trace for changed decisions.
8. Review rendered output.

No developer should need to inspect an optional prebuilt binary bundle directly.
Use normalized debug JSON and trace output.

## Initial Implementation Plan

Phase 1: Core skeleton

- Define request, date facts, catalog stubs, trace, and document model.
- Implement Gregorian date math and Easter.
- Implement Roman 1960 profile shell.
- Implement Lauds ordinary template.
- Implement minimal psalter fixture.
- Resolve one known date: 2026-06-15 Lauds.

Phase 2: Data loader

- Parse YAML.
- Validate IDs and references.
- Build the typed in-memory catalog.
- Optionally emit normalized catalog debug JSON.
- Load catalog in core.

Phase 3: Roman 1960 general calendar

- Temporal candidate generation.
- Sanctoral fixed dates.
- Occurrence.
- Commemoration.
- Lauds, Vespers, and little hours.

Phase 4: Full office structure

- Matins.
- Concurrence.
- More psalter variants.
- Common resolution.
- Collect conclusion policies.

Phase 5: Migration breadth

- Import Divinum Officium data.
- Add comparison harness.
- Add more profiles.
- Add local calendars.
- Add chant metadata.

## Acceptance Criteria

The architecture is implemented correctly when:

- A request can be resolved without mutable global state.
- Every principal office has an occurrence or concurrence trace.
- Every commemoration has a commemoration trace.
- Every text section has provenance.
- Renderers can be replaced without changing liturgical decisions.
- The startup loader catches broken references before serving requests.
- Roman 1960 fixture dates match accepted expected output.
- Differences from Divinum Officium are either fixed or documented as intentional.

## Glossary

`Candidate`: An observance that might apply to a date before precedence.

`Occurrence`: The decision between observances on the same date for non-vesperal
hours.

`Concurrence`: The decision between today's second Vespers and tomorrow's first
Vespers.

`Principal`: The winning observance whose office is prayed.

`Commemoration`: A secondary observance remembered with antiphon, versicle, and
collect where the profile permits.

`Profile`: A complete rubrical configuration, such as Roman 1960.

`Ordinary`: The structural template of an hour.

`Proper`: Text belonging directly to an observance.

`Common`: Reusable text for a class of observance.

`Psalter policy`: The rule that selects psalms and antiphons.

`Trace`: A structured explanation of decisions made by the engine.
