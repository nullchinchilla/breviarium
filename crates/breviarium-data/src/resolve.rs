//! Office resolution: turn a date + hour into a structured [`OfficeDocument`].
//!
//! Each hour is a data-driven [`Slot`] table (replacing the old `builtin_steps`
//! + 35-variant skeleton); resolution selects the day's observances, builds the
//! source/book context, and fills each slot from the book stack, expanding the
//! stored (already-typed) lexicon nodes into render-neutral [`DocumentNode`]s.

use crate::calendar::{divinum_weekday_number, sanctoral_key};
use crate::catalog::section_nodes;
use crate::data_slug;
use crate::*;
use chrono::{Datelike, Duration, NaiveDate, Weekday};
use std::collections::{BTreeMap, BTreeSet};

/// One structural shape of office content. A handful of these replace the 35
/// `OfficeStepKind` variants of the original resolver.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Handler {
    /// Fixed text assembled from the ordinary book (openings, Pretiosa,
    /// examination, conclusions). `Slot::arg` selects which formula.
    Formula,
    /// Matins invitatory: Psalm 94 woven with the invitatory antiphon.
    Invitatory,
    /// Antiphon(s) + psalms; ferial psalms come from the psalter by weekday.
    /// `Slot::arg` selects the scheme (`lauds`/`vespers`/`minor`/`compline`).
    Psalmody,
    /// Matins nocturns, lessons and responsories (3-vs-9 lesson logic).
    Matins,
    /// Generic single-slot fill from the book stack (Prime/Compline short reading).
    Lookup,
    /// The hymn for an hour. `Slot::arg` is `matins`/`prime`/`minor`/`compline`.
    Hymn,
    /// Composite chapter+hymn+verse (major hours) or chapter+responsory+verse
    /// (minor hours / Prime / Compline). `Slot::arg` is `major` or `minor`.
    ChapterBlock,
    /// Gospel canticle + antiphon. `Slot::arg` is `benedictus` / `magnificat` /
    /// `nunc-dimittis`.
    Canticle,
    /// Domine exaudi + Oremus + collect (candidate order) + commemorations.
    /// `Slot::arg` is `standard` / `prime` / `compline`.
    Collect,
    /// Prime martyrology (next calendar day).
    Martyrology,
    /// Preces omission marker for Roman 1960.
    Preces,
    /// Compline Marian antiphon, selected by season.
    FinalAntiphon,
}

/// One ordered slot in an hour's template. Mirrors the old `RawOfficeStep`
/// (`id`, `role`, per-language `title`) but dispatches on a generic [`Handler`].
#[derive(Clone, Debug)]
pub(crate) struct Slot {
    /// Stable block id suffix (`office.{profile}.{hour}.{id}`).
    pub id: &'static str,
    /// Semantic role carried onto the resolved block.
    pub role: TextRole,
    /// Which structural handler fills this slot.
    pub handler: Handler,
    /// Handler argument: canonical slot name, formula key, scheme, or canticle.
    pub arg: &'static str,
    /// Latin column title.
    pub title_la: &'static str,
    /// English column title.
    pub title_en: &'static str,
}

const fn slot(
    id: &'static str,
    role: TextRole,
    handler: Handler,
    arg: &'static str,
    title_la: &'static str,
    title_en: &'static str,
) -> Slot {
    Slot {
        id,
        role,
        handler,
        arg,
        title_la,
        title_en,
    }
}

use Handler::*;
use TextRole as R;

const MATINS: &[Slot] = &[
    slot(
        "opening",
        R::Opening,
        Formula,
        "matins-opening",
        "Incipit",
        "Start",
    ),
    slot(
        "invitatory",
        R::Invitatory,
        Invitatory,
        "matins-invitatory",
        "Invitatorium",
        "Invitatory",
    ),
    slot("hymn", R::Hymn, Hymn, "matins-hymn", "Hymnus", "Hymn"),
    slot(
        "nocturns",
        R::Reading,
        Matins,
        "matins",
        "Nocturni",
        "Nocturns",
    ),
];

const LAUDS: &[Slot] = &[
    slot(
        "opening",
        R::Opening,
        Formula,
        "opening",
        "Incipit",
        "Start",
    ),
    slot(
        "psalmody",
        R::Psalmody,
        Psalmody,
        "lauds",
        "Psalmi",
        "Psalms",
    ),
    slot(
        "chapter",
        R::Chapter,
        ChapterBlock,
        "major",
        "Capitulum Hymnus Versus",
        "Chapter Hymn Verse",
    ),
    slot(
        "benedictus",
        R::GospelCanticle,
        Canticle,
        "benedictus",
        "Canticum: Benedictus",
        "Canticle: Benedictus",
    ),
    slot("preces", R::Preces, Preces, "", "Preces", "Preces"),
    slot(
        "collect",
        R::Collect,
        Collect,
        "standard",
        "Oratio",
        "Prayer",
    ),
    slot(
        "conclusion",
        R::Conclusion,
        Formula,
        "conclusion",
        "Conclusio",
        "Conclusion",
    ),
];

const PRIME: &[Slot] = &[
    slot(
        "opening",
        R::Opening,
        Formula,
        "opening",
        "Incipit",
        "Start",
    ),
    slot("hymn", R::Hymn, Hymn, "prime-hymn", "Hymnus", "Hymn"),
    slot(
        "psalmody",
        R::Psalmody,
        Psalmody,
        "minor",
        "Psalmi",
        "Psalms",
    ),
    slot(
        "chapter",
        R::Chapter,
        ChapterBlock,
        "minor",
        "Capitulum",
        "Chapter",
    ),
    slot("collect", R::Collect, Collect, "prime", "Oratio", "Prayer"),
    slot(
        "martyrology",
        R::MartyrologyEntry,
        Martyrology,
        "",
        "Martyrologium",
        "Martyrology",
    ),
    slot(
        "pretiosa",
        R::Versicle,
        Formula,
        "pretiosa",
        "Pretiosa",
        "Pretiosa",
    ),
    slot(
        "chapter-office",
        R::Chapter,
        Formula,
        "chapter-office",
        "Capitulum",
        "Chapter Office",
    ),
    slot(
        "short-reading",
        R::ShortReading,
        Lookup,
        "prime-short-reading",
        "Lectio brevis",
        "Short Reading",
    ),
    slot(
        "conclusion",
        R::Conclusion,
        Formula,
        "prime-conclusion",
        "Conclusio",
        "Conclusion",
    ),
];

const MINOR: &[Slot] = &[
    slot(
        "opening",
        R::Opening,
        Formula,
        "opening",
        "Incipit",
        "Start",
    ),
    slot("hymn", R::Hymn, Hymn, "minor-hymn", "Hymnus", "Hymn"),
    slot(
        "psalmody",
        R::Psalmody,
        Psalmody,
        "minor",
        "Psalmi",
        "Psalms",
    ),
    slot(
        "chapter",
        R::Chapter,
        ChapterBlock,
        "minor",
        "Capitulum",
        "Chapter",
    ),
    slot("preces", R::Preces, Preces, "", "Preces", "Preces"),
    slot(
        "collect",
        R::Collect,
        Collect,
        "daytime",
        "Oratio",
        "Prayer",
    ),
    slot(
        "conclusion",
        R::Conclusion,
        Formula,
        "conclusion",
        "Conclusio",
        "Conclusion",
    ),
];

const VESPERS: &[Slot] = &[
    slot(
        "opening",
        R::Opening,
        Formula,
        "opening",
        "Incipit",
        "Start",
    ),
    slot(
        "psalmody",
        R::Psalmody,
        Psalmody,
        "vespers",
        "Psalmi",
        "Psalms",
    ),
    slot(
        "chapter",
        R::Chapter,
        ChapterBlock,
        "major",
        "Capitulum Hymnus Versus",
        "Chapter Hymn Verse",
    ),
    slot(
        "magnificat",
        R::GospelCanticle,
        Canticle,
        "magnificat",
        "Canticum: Magnificat",
        "Canticle: Magnificat",
    ),
    slot("preces", R::Preces, Preces, "", "Preces", "Preces"),
    slot(
        "collect",
        R::Collect,
        Collect,
        "standard",
        "Oratio",
        "Prayer",
    ),
    slot(
        "conclusion",
        R::Conclusion,
        Formula,
        "conclusion",
        "Conclusio",
        "Conclusion",
    ),
];

const COMPLINE: &[Slot] = &[
    slot(
        "opening",
        R::Opening,
        Formula,
        "compline-opening",
        "Benedictio",
        "Blessing",
    ),
    slot(
        "short-reading",
        R::ShortReading,
        Lookup,
        "compline-short-reading",
        "Lectio brevis",
        "Short Reading",
    ),
    slot(
        "examination",
        R::Preces,
        Formula,
        "examination",
        "Examen",
        "Examination",
    ),
    slot(
        "opening-2",
        R::Opening,
        Formula,
        "opening",
        "Incipit",
        "Start",
    ),
    slot(
        "psalmody",
        R::Psalmody,
        Psalmody,
        "compline",
        "Psalmi",
        "Psalms",
    ),
    slot("hymn", R::Hymn, Hymn, "compline-hymn", "Hymnus", "Hymn"),
    slot(
        "chapter",
        R::Chapter,
        ChapterBlock,
        "minor",
        "Capitulum",
        "Chapter",
    ),
    slot(
        "nunc-dimittis",
        R::GospelCanticle,
        Canticle,
        "nunc-dimittis",
        "Canticum: Nunc dimittis",
        "Canticle: Nunc dimittis",
    ),
    slot(
        "collect",
        R::Collect,
        Collect,
        "compline",
        "Oratio",
        "Prayer",
    ),
    slot(
        "conclusion",
        R::Conclusion,
        Formula,
        "compline-conclusion",
        "Conclusio",
        "Conclusion",
    ),
    slot(
        "final-antiphon",
        R::MarianAntiphon,
        FinalAntiphon,
        "final-antiphon",
        "Antiphona finalis",
        "Final Antiphon",
    ),
];

/// The ordered slot template for an hour — the data-driven office skeleton.
pub(crate) fn hour_slots(hour: Hour) -> &'static [Slot] {
    match hour {
        Hour::Matins => MATINS,
        Hour::Lauds => LAUDS,
        Hour::Prime => PRIME,
        Hour::Terce | Hour::Sext | Hour::None => MINOR,
        Hour::Vespers => VESPERS,
        Hour::Compline => COMPLINE,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::slots;

    const ALL_HOURS: [Hour; 8] = [
        Hour::Matins,
        Hour::Lauds,
        Hour::Prime,
        Hour::Terce,
        Hour::Sext,
        Hour::None,
        Hour::Vespers,
        Hour::Compline,
    ];

    #[test]
    fn every_hour_has_a_template_starting_with_opening() {
        for hour in ALL_HOURS {
            let template = hour_slots(hour);
            assert!(!template.is_empty(), "{hour:?} has no slots");
            assert_eq!(
                template[0].id, "opening",
                "{hour:?} must open with `opening`"
            );
        }
    }

    #[test]
    fn templates_match_builtin_step_counts() {
        // Counts mirror the original `builtin_steps`, guarding the table against
        // accidental drift while the old path still exists.
        assert_eq!(hour_slots(Hour::Matins).len(), 4);
        assert_eq!(hour_slots(Hour::Lauds).len(), 7);
        assert_eq!(hour_slots(Hour::Prime).len(), 10);
        assert_eq!(hour_slots(Hour::Terce).len(), 7);
        assert_eq!(hour_slots(Hour::Vespers).len(), 7);
        assert_eq!(hour_slots(Hour::Compline).len(), 11);
    }

    #[test]
    fn terce_sext_none_share_one_template() {
        let terce = hour_slots(Hour::Terce);
        assert!(std::ptr::eq(terce, hour_slots(Hour::Sext)));
        assert!(std::ptr::eq(terce, hour_slots(Hour::None)));
    }

    #[test]
    fn lookup_and_named_handlers_reference_canonical_slots() {
        // Lookup/Invitatory/FinalAntiphon args must be real canonical slots, so
        // the table and the importer's emitted slot vocabulary cannot drift.
        for hour in ALL_HOURS {
            for s in hour_slots(hour) {
                if matches!(
                    s.handler,
                    Handler::Lookup | Handler::Invitatory | Handler::FinalAntiphon
                ) {
                    assert!(
                        slots::CANONICAL.contains(&s.arg),
                        "{hour:?}/{} uses non-canonical slot `{}`",
                        s.id,
                        s.arg
                    );
                }
            }
        }
    }
}

// ==== resolver (moved from lib.rs) ====
pub(crate) fn resolve_office(
    catalog: &Catalog,
    request: OfficeRequest,
) -> Result<OfficeDocument, DataError> {
    let mut diagnostics = Vec::new();
    let mut trace = Vec::new();
    if catalog.profile(&request.profile).is_none() {
        return Err(DataError::UnsupportedScope {
            message: format!("profile `{}` is not embedded", request.profile),
        });
    }

    let facts = office_date_facts(request.date)?;
    let primary_language = "la";
    let temporal_key = first_existing_source_key(
        catalog,
        primary_language,
        temporal_source_candidates(&facts),
    );
    let sanctoral_key = first_existing_source_key(
        catalog,
        primary_language,
        sanctoral_source_candidates(catalog, primary_language, &facts.sanctoral_key),
    );
    trace.push(TraceEvent {
        phase: "date",
        message: format!(
            "{} -> temporal `{}` sanctoral `{}`",
            facts.date, facts.temporal_stem, facts.sanctoral_key
        ),
    });

    let temporal = temporal_key.as_deref().map(|key| {
        observance_from_source_key(
            catalog,
            primary_language,
            key,
            ObservanceKind::Temporal,
            format!("temporal.{}", facts.temporal_stem.to_ascii_lowercase()),
        )
    });
    let sanctoral = sanctoral_key.as_deref().map(|key| {
        observance_from_source_key(
            catalog,
            primary_language,
            key,
            ObservanceKind::Sanctoral,
            format!("sanctoral.{}", facts.sanctoral_key),
        )
    });
    let principal = choose_principal(temporal.as_ref(), sanctoral.as_ref(), &facts, &mut trace)
        .ok_or_else(|| DataError::MissingText {
            message: format!("no temporal or sanctoral source for {}", facts.date),
        })?;
    let mut commemorations = Vec::new();
    if matches!(request.hour, Hour::Lauds | Hour::Vespers) {
        if let Some(other) =
            non_principal_candidate(&principal, temporal.as_ref(), sanctoral.as_ref())
        {
            let principal_rank = principal.rank.unwrap_or(0.0);
            let other_rank = other.rank.unwrap_or(0.0);
            if principal_rank < 6.0 && other_rank > 0.0 && other_rank < 6.0 {
                commemorations.push(other.clone());
            }
        }
    }
    let context = OfficeContext::new(
        catalog,
        &facts,
        request.hour,
        &request.profile,
        &principal,
        temporal_key,
        sanctoral_key,
        &commemorations,
        primary_language,
    );
    let blocks = execute_steps(catalog, &request, &context, &mut diagnostics);
    Ok(OfficeDocument {
        date_facts: facts,
        hour: request.hour,
        profile: request.profile,
        principal,
        temporal,
        sanctoral,
        commemorations,
        blocks,
        diagnostics,
        trace,
    })
}

// The fixed structural books (ferial/seasonal defaults), referenced by key.
const MATINS_SPECIAL: &str = "ordinary/matins";
const PSALTER_MAJOR: &str = "psalter/major";
const PSALTER_MINOR: &str = "psalter/minor";
const PSALTER_MATINS: &str = "psalter/matins";
const BENEDICTIONS: &str = "ordinary/benedictions";

/// An ordered pile of books (source keys), highest priority first. The single
/// generic slot-filler: every `*_sources()` helper and the repeated
/// `first_section_doc(...).or_else(...)` orchestration becomes `stack.doc(slot)`.
struct Stack {
    keys: Vec<String>,
}

impl Stack {
    /// Builds a stack from ordered keys, dropping empties and de-duplicating.
    fn of<I, S>(keys: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut out: Vec<String> = Vec::new();
        for key in keys {
            let key = key.into();
            if !key.is_empty() && !out.iter().any(|existing| *existing == key) {
                out.push(key);
            }
        }
        Self { keys: out }
    }

    /// Fills the first of `slots` that resolves in any book (slot-major order,
    /// matching the legacy `first_of_sections`).
    fn of_slots(
        &self,
        catalog: &Catalog,
        language: &str,
        slots: &[&str],
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Result<Vec<DocumentNode>, String> {
        for slot in slots {
            for key in &self.keys {
                if let Some(nodes) = section_nodes(catalog, language, key, slot) {
                    return expand_nodes(catalog, language, &nodes, diagnostics);
                }
            }
        }
        Err(format!("missing sections {slots:?}"))
    }

    /// Fills a single slot from the first book that has it.
    fn doc(
        &self,
        catalog: &Catalog,
        language: &str,
        slot: &str,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Result<Vec<DocumentNode>, String> {
        self.of_slots(catalog, language, &[slot], diagnostics)
    }

    /// The antiphon strings of the first book providing `slot`.
    fn antiphons(&self, catalog: &Catalog, language: &str, slot: &str) -> Option<Vec<String>> {
        self.keys
            .iter()
            .find_map(|key| section_antiphons(catalog, language, key, slot))
    }

    /// The psalmody entries of the first book providing `slot`.
    fn psalmody(
        &self,
        catalog: &Catalog,
        language: &str,
        slot: &str,
    ) -> Option<Vec<PsalmodyEntry>> {
        self.keys
            .iter()
            .find_map(|key| section_psalmody(catalog, language, key, slot))
    }

    /// Whether any book in the stack provides `slot`.
    fn has(&self, catalog: &Catalog, language: &str, slot: &str) -> bool {
        self.keys
            .iter()
            .any(|key| section_nodes(catalog, language, key, slot).is_some())
    }
}

#[derive(Clone, Debug)]
struct OfficeContext {
    facts: DateFacts,
    hour: Hour,
    profile: ProfileId,
    principal_key: Option<String>,
    temporal_key: Option<String>,
    weekly_temporal_key: Option<String>,
    previous_temporal_key: Option<String>,
    scripture_key: Option<String>,
    commune_key: Option<String>,
    collect_reference_keys: Vec<String>,
    commemorations: Vec<CommemorationContext>,
    rule_flags: BTreeSet<String>,
    rule_values: BTreeMap<String, String>,
    laudes: usize,
}

#[derive(Clone, Debug)]
struct CommemorationContext {
    source_key: String,
    commune_key: Option<String>,
}

impl OfficeContext {
    #[allow(clippy::too_many_arguments)]
    fn new(
        catalog: &Catalog,
        facts: &DateFacts,
        hour: Hour,
        profile: &str,
        principal: &OfficeObservance,
        temporal_key: Option<String>,
        _sanctoral_key: Option<String>,
        commemorations: &[OfficeObservance],
        primary_language: &str,
    ) -> Self {
        let principal_key = principal.catalog_key.clone();
        let weekly_temporal_key = first_existing_source_key(
            catalog,
            primary_language,
            weekly_temporal_source_candidates(facts),
        );
        let previous_temporal_key =
            adjacent_temporal_key(catalog, primary_language, facts.date - Duration::days(1));
        let scripture_key = first_existing_source_key(
            catalog,
            primary_language,
            scripture_source_candidates(facts),
        );
        let principal_office = principal_key.as_deref().and_then(|key| catalog.office(key));
        let rule_flags: BTreeSet<String> = principal_office
            .map(|office| office.flags.clone())
            .unwrap_or_default();
        let rule_values: BTreeMap<String, String> = principal_office
            .map(|office| office.values.clone())
            .unwrap_or_default();
        // `office.common` is already a canonical `book/office` key (resolved by
        // the importer).
        let commune_key = principal_office.and_then(|office| office.common.clone());
        let collect_reference_keys = collect_reference_keys_for_sources(
            catalog,
            primary_language,
            &[
                principal_key.as_deref(),
                commune_key.as_deref(),
                temporal_key.as_deref(),
                weekly_temporal_key.as_deref(),
                previous_temporal_key.as_deref(),
            ],
        );
        let commemorations = commemorations
            .iter()
            .filter_map(|commemoration| {
                let source_key = commemoration.catalog_key.clone()?;
                let commune_key = catalog
                    .office(&source_key)
                    .and_then(|office| office.common.clone());
                Some(CommemorationContext {
                    source_key,
                    commune_key,
                })
            })
            .collect();
        let laudes = if rule_values
            .get("laudes")
            .is_some_and(|value| value.trim() == "2")
            || rule_flags.contains("laudes-2")
        {
            2
        } else {
            1
        };
        Self {
            facts: facts.clone(),
            hour,
            profile: profile.to_string(),
            principal_key,
            temporal_key,
            weekly_temporal_key,
            previous_temporal_key,
            scripture_key,
            commune_key,
            collect_reference_keys,
            commemorations,
            rule_flags,
            rule_values,
            laudes,
        }
    }

    /// The proper stack: the day's own office and the common it points to.
    fn principal(&self) -> Stack {
        Stack::of(
            [&self.principal_key, &self.commune_key]
                .into_iter()
                .flatten()
                .cloned(),
        )
    }

    /// The inherited stack: proper, common, then the temporal fallbacks.
    fn inherited(&self) -> Stack {
        Stack::of(
            [
                &self.principal_key,
                &self.commune_key,
                &self.temporal_key,
                &self.weekly_temporal_key,
                &self.previous_temporal_key,
            ]
            .into_iter()
            .flatten()
            .cloned(),
        )
    }

    /// The collect stack: inherited, then any extra collect-reference sources.
    fn collect(&self) -> Stack {
        let mut stack = self.inherited();
        for key in &self.collect_reference_keys {
            if !stack.keys.iter().any(|existing| existing == key) {
                stack.keys.push(key.clone());
            }
        }
        stack
    }

    /// The Matins-lesson stack: proper, monthly Scripture, temporal, common.
    fn matins_lessons(&self) -> Stack {
        Stack::of(
            [
                &self.principal_key,
                &self.scripture_key,
                &self.temporal_key,
                &self.weekly_temporal_key,
                &self.commune_key,
            ]
            .into_iter()
            .flatten()
            .cloned(),
        )
    }

    fn has_rule(&self, id: &str) -> bool {
        let id = normalize_rule_id(id);
        self.rule_flags.contains(&id)
            || self.rule_values.contains_key(&id)
            || self
                .rule_flags
                .iter()
                .any(|flag| flag.contains(&id) || id.contains(flag))
    }

    fn omits_optional_psalms(&self) -> bool {
        self.profile == "roman-1960"
    }
}

fn execute_steps(
    catalog: &Catalog,
    request: &OfficeRequest,
    context: &OfficeContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<OfficeBlock> {
    // The hour's structure is the data-driven `Slot` table; each slot dispatches
    // on its generic `Handler` directly to a resolution function.
    hour_slots(request.hour)
        .iter()
        .map(|slot| {
            let columns = request
                .languages
                .iter()
                .map(|language| {
                    let content =
                        match dispatch(catalog, language, context, slot, request.hour, diagnostics)
                        {
                            Ok(nodes) => OfficeColumnContent::Resolved { nodes },
                            Err(reason) => OfficeColumnContent::Missing { reason },
                        };
                    OfficeColumn {
                        language: language.clone(),
                        title: slot_title(slot, language),
                        content,
                    }
                })
                .collect();
            OfficeBlock {
                id: format!(
                    "office.{}.{}.{}",
                    request.profile,
                    request.hour.as_str(),
                    slot.id
                ),
                role: slot.role.clone(),
                columns,
            }
        })
        .collect()
}

/// Per-language column title for a slot, matching the old `step.titles` map
/// which only ever carried `la`/`en` keys (other languages → `None`).
fn slot_title(slot: &Slot, language: &str) -> Option<String> {
    match language {
        "la" => Some(slot.title_la.to_string()),
        "en" => Some(slot.title_en.to_string()),
        _ => None,
    }
}

/// Fills one slot by dispatching its generic [`Handler`] (and `arg`) directly to
/// a resolution function. This is the whole step-resolution surface — there is no
/// intermediate per-hour `OfficeStepKind` enum.
fn dispatch(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    slot: &Slot,
    hour: Hour,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    use Handler::*;
    match (slot.handler, slot.arg) {
        (Formula, arg) => resolve_formula(catalog, language, context, arg, diagnostics),
        (Invitatory, _) => resolve_matins_invitatory(catalog, language, context, diagnostics),
        (Lookup, "prime-short-reading") => {
            resolve_prime_short_reading(catalog, language, context, diagnostics)
        }
        (Lookup, "compline-short-reading") => {
            Stack::of(["ordinary/compline"]).doc(catalog, language, "short-reading", diagnostics)
        }
        (Lookup, _) => Err("unsupported step".to_string()),
        (Hymn, arg) => resolve_hymn(catalog, language, context, arg, diagnostics),
        (Psalmody, arg) => resolve_psalmody(catalog, language, context, arg, diagnostics),
        (Matins, _) => resolve_matins_nocturns(catalog, language, context, diagnostics),
        (ChapterBlock, arg) => {
            resolve_chapter_block(catalog, language, context, arg, hour, diagnostics)
        }
        (Canticle, which) => resolve_canticle(catalog, language, context, which, diagnostics),
        (Collect, arg) => resolve_collect(catalog, language, context, arg, diagnostics),
        (Martyrology, _) => resolve_prime_martyrology(catalog, language, context, diagnostics),
        (Preces, _) => Ok(vec![DocumentNode::Marker {
            text: localized_literal(language, "omittitur", "omit").to_string(),
        }]),
        (FinalAntiphon, _) => resolve_final_antiphon(catalog, language, context, diagnostics),
    }
}

/// One element of a fixed formula assembly (see [`FORMULA_ORDER`]).
enum Part {
    /// A named formula from the ordinary book.
    Formula(&'static str),
    /// The first of several named formulae that resolves.
    FirstOf(&'static [&'static str]),
    /// Splice in another assembly (e.g. matins-opening includes opening).
    Include(&'static str),
    /// `Deus in adjutorium` then the seasonal Alleluia / Lent line.
    Opening,
    /// `Domine, exaudi` versicle + response.
    DomineExaudi,
    /// The Compline examination rubric.
    ExaminationRubric,
    /// An Amen.
    Amen,
}

/// Each hour's fixed opening / Pretiosa / examination / conclusion, as data.
const FORMULA_ORDER: &[(&str, &[Part])] = {
    use Part::*;
    &[
        ("opening", &[Opening]),
        (
            "matins-opening",
            &[Formula("domine-labia"), Include("opening")],
        ),
        (
            "compline-opening",
            &[
                Formula("jube-domne"),
                Formula("benedictio-completorium"),
                Amen,
            ],
        ),
        (
            "examination",
            &[
                Formula("adjutorium-nostrum"),
                ExaminationRubric,
                Formula("pater-noster"),
                Formula("confiteor"),
                Formula("misereatur"),
                Formula("indulgentiam"),
                Formula("converte-nos"),
            ],
        ),
        ("pretiosa", &[Formula("pretiosa")]),
        (
            "chapter-office",
            &[
                Formula("deus-in-adjutorium-iij"),
                Formula("gloria"),
                Formula("kyrie"),
                Formula("pater-noster-et"),
                Formula("respice"),
                Formula("oremus"),
                Formula("dirigere"),
            ],
        ),
        (
            "prime-conclusion",
            &[
                Formula("adjutorium-nostrum"),
                Formula("benedicite"),
                Formula("benedictio-prima2"),
            ],
        ),
        (
            "compline-conclusion",
            &[
                DomineExaudi,
                Formula("benedicamus-domino"),
                FirstOf(&["benedictio-completorium-final", "benedictio-completorium2"]),
                Amen,
            ],
        ),
        (
            "conclusion",
            &[
                DomineExaudi,
                Formula("benedicamus-domino"),
                Formula("fidelium-animae"),
            ],
        ),
    ]
};

/// Assembles a fixed sequence of ordinary-book formulae from [`FORMULA_ORDER`].
fn resolve_formula(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    arg: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let parts = FORMULA_ORDER
        .iter()
        .find(|(name, _)| *name == arg)
        .or_else(|| FORMULA_ORDER.iter().find(|(name, _)| *name == "conclusion"))
        .map(|(_, parts)| *parts)
        .unwrap_or(&[]);
    let mut nodes = Vec::new();
    for part in parts {
        match part {
            Part::Formula(name) => {
                nodes.extend(formula_nodes(catalog, language, name, diagnostics)?)
            }
            Part::FirstOf(names) => {
                nodes.extend(first_formula_doc(catalog, language, names, diagnostics)?)
            }
            Part::Include(other) => nodes.extend(resolve_formula(
                catalog,
                language,
                context,
                other,
                diagnostics,
            )?),
            Part::Opening => {
                nodes.extend(formula_nodes(
                    catalog,
                    language,
                    "deus-in-adjutorium",
                    diagnostics,
                )?);
                let alleluia = formula_lines(catalog, language, "alleluia", diagnostics)?;
                let index = usize::from(context.facts.temporal_week.starts_with("Quad"));
                if let Some(line) = alleluia.get(index) {
                    nodes.push(DocumentNode::Text { text: line.clone() });
                }
            }
            Part::DomineExaudi => nodes.extend(domine_exaudi_nodes(language)),
            Part::ExaminationRubric => nodes.push(DocumentNode::Rubric {
                text: localized_literal(
                    language,
                    "Examen conscientiae vel Pater Noster totum secreto.",
                    "There follows an examination of conscience, or the Our Father said silently.",
                )
                .to_string(),
            }),
            Part::Amen => nodes.extend(amen_nodes()),
        }
    }
    Ok(nodes)
}

fn resolve_matins_invitatory(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let antiphon = context
        .principal()
        .antiphons(catalog, language, "matins-invitatory")
        .or_else(|| Stack::of([MATINS_SPECIAL]).antiphons(catalog, language, "invit"))
        .and_then(|values| values.into_iter().next())
        .unwrap_or_default();
    let mut nodes = Vec::new();
    if !antiphon.is_empty() {
        nodes.push(DocumentNode::Text {
            text: format!("Ant. {antiphon}"),
        });
    }
    nodes.extend(psalm_nodes(
        catalog,
        language,
        &PsalmReference {
            number: "94".to_string(),
            start: None,
            end: None,
            optional: false,
        },
        diagnostics,
    )?);
    if !antiphon.is_empty() {
        nodes.push(DocumentNode::Text {
            text: format!("Ant. {}", close_antiphon(&antiphon)),
        });
    }
    Ok(nodes)
}

/// The hymn for an hour: proper if present, else the season/day hymn from the
/// relevant special book. `arg` is `matins`/`prime`/`minor`/`compline`.
fn resolve_hymn(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    arg: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    match arg {
        "matins-hymn" => context
            .principal()
            .doc(catalog, language, "matins-hymn", diagnostics)
            .or_else(|_| {
                Stack::of([MATINS_SPECIAL]).doc(
                    catalog,
                    language,
                    &matins_ordinary_hymn_section(context),
                    diagnostics,
                )
            }),
        "prime-hymn" => {
            Stack::of(["ordinary/prime-fixed"]).doc(catalog, language, "hymn", diagnostics)
        }
        "minor-hymn" => {
            let hour =
                canonical_minor_hour(context.hour).ok_or_else(|| "not a minor hour".to_string())?;
            Stack::of(["ordinary/minor-hymn"]).doc(
                catalog,
                language,
                &format!("{hour}-hymn"),
                diagnostics,
            )
        }
        // "compline-hymn": today's seasonal Compline book, else the default.
        _ => {
            let season = if context.facts.temporal_week.starts_with("Quad5") {
                Some("quad5")
            } else if context.facts.temporal_week.starts_with("Quad") {
                Some("quad")
            } else if context.facts.temporal_week.starts_with("Pasc") {
                Some("pasch")
            } else {
                None
            };
            let mut keys = Vec::new();
            if let Some(season) = season {
                keys.push(format!("ordinary/compline-{season}"));
            }
            keys.push("ordinary/compline".to_string());
            Stack::of(keys).doc(catalog, language, "hymn", diagnostics)
        }
    }
}

fn resolve_matins_nocturns(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let entries = context
        .principal()
        .psalmody(catalog, language, "matins-psalmody")
        .or_else(|| {
            Stack::of([PSALTER_MATINS]).psalmody(
                catalog,
                language,
                &format!("day{}", divinum_weekday_number(context.facts.weekday)),
            )
        })
        .ok_or_else(|| "missing Matins psalmody".to_string())?;
    let mut nodes = Vec::new();
    let lesson_count = matins_lesson_count(catalog, language, context);
    if lesson_count <= 3 {
        nodes.push(DocumentNode::Marker {
            text: localized_nocturn_title(language, 1),
        });
        for entry in &entries {
            nodes.extend(expand_psalmody_entry(
                catalog,
                language,
                entry,
                diagnostics,
            )?);
        }
        let versicle_nocturn = ((entries.len() + 2) / 3).max(1);
        match resolve_matins_versicle(catalog, language, context, versicle_nocturn, diagnostics) {
            Ok(versicle) => nodes.extend(versicle),
            Err(reason) => nodes.push(DocumentNode::Unresolved {
                kind: "section".to_string(),
                value: format!("Nocturn {versicle_nocturn} Versum"),
                reason,
            }),
        }
        nodes.extend(resolve_matins_lessons(
            catalog,
            language,
            context,
            1,
            1,
            3,
            diagnostics,
        )?);
        return Ok(nodes);
    }
    for (index, entry) in entries.iter().enumerate() {
        if index % 3 == 0 {
            nodes.push(DocumentNode::Marker {
                text: localized_nocturn_title(language, index / 3 + 1),
            });
        }
        nodes.extend(expand_psalmody_entry(
            catalog,
            language,
            entry,
            diagnostics,
        )?);
        if index % 3 == 2 {
            let nocturn = index / 3 + 1;
            match resolve_matins_versicle(catalog, language, context, nocturn, diagnostics) {
                Ok(versicle) => nodes.extend(versicle),
                Err(reason) => nodes.push(DocumentNode::Unresolved {
                    kind: "section".to_string(),
                    value: format!("Nocturn {nocturn} Versum"),
                    reason,
                }),
            }
            nodes.extend(resolve_matins_lessons(
                catalog,
                language,
                context,
                nocturn,
                (nocturn - 1) * 3 + 1,
                3,
                diagnostics,
            )?);
        }
    }
    Ok(nodes)
}

fn matins_lesson_count(catalog: &Catalog, language: &str, context: &OfficeContext) -> usize {
    if has_abbreviated_sanctoral_lesson(catalog, language, context) {
        return 3;
    }
    if context.has_rule("9-lectiones") {
        return 9;
    }
    if context
        .principal()
        .has(catalog, language, "matins-reading-4")
    {
        9
    } else {
        3
    }
}

fn has_abbreviated_sanctoral_lesson(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
) -> bool {
    let principal = context.principal();
    principal.has(catalog, language, "matins-reading-3-abbreviated")
        && !principal.has(catalog, language, "matins-reading-4")
}

fn resolve_matins_versicle(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    nocturn: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let section = format!("matins-nocturn-{nocturn}-versicle");
    context
        .principal()
        .doc(catalog, language, &section, diagnostics)
        .or_else(|_| {
            let pairs = section_nodes(
                catalog,
                language,
                PSALTER_MATINS,
                &format!("day{}", divinum_weekday_number(context.facts.weekday)),
            )
            .unwrap_or_default()
            .into_iter()
            .filter_map(|node| match node {
                ContentNode::Versicle { text } => Some(DocumentNode::Versicle { text }),
                ContentNode::Response { text } => Some(DocumentNode::Response { text }),
                _ => None,
            })
            .collect::<Vec<_>>();
            let nodes = pairs
                .into_iter()
                .skip((nocturn - 1) * 2)
                .take(2)
                .collect::<Vec<_>>();
            if nodes.is_empty() {
                Err(format!("missing {section}"))
            } else {
                Ok(nodes)
            }
        })
}

fn resolve_matins_lessons(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    nocturn: usize,
    first: usize,
    count: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let mut nodes = Vec::new();
    nodes.extend(resolve_matins_absolution(
        catalog,
        language,
        nocturn,
        diagnostics,
    )?);
    let lessons = context.matins_lessons();
    let use_abbreviated_sanctoral_lesson =
        has_abbreviated_sanctoral_lesson(catalog, language, context);
    for lesson in first..first + count {
        nodes.extend(resolve_matins_blessing(
            catalog,
            language,
            context,
            nocturn,
            lesson,
            diagnostics,
        )?);
        nodes.push(DocumentNode::Marker {
            text: localized_lesson_title(language, lesson),
        });
        let section = format!("matins-reading-{lesson}");
        let mut lesson_added = false;
        if context.has_rule("lectio1-tempnat") && lesson <= 3 {
            if let Some(key) = &context.temporal_key {
                if let Ok(lectio) =
                    Stack::of([key.clone()]).doc(catalog, language, &section, diagnostics)
                {
                    nodes.extend(lectio);
                    lesson_added = true;
                }
            }
        }
        if !lesson_added && use_abbreviated_sanctoral_lesson && lesson == 3 {
            if let Ok(lectio) = context.principal().doc(
                catalog,
                language,
                "matins-reading-3-abbreviated",
                diagnostics,
            ) {
                nodes.extend(lectio);
                lesson_added = true;
            }
        }
        if !lesson_added {
            match lessons.doc(catalog, language, &section, diagnostics) {
                Ok(lectio) => nodes.extend(lectio),
                Err(reason) => nodes.push(DocumentNode::Unresolved {
                    kind: "section".to_string(),
                    value: section,
                    reason,
                }),
            }
        }
        if let Ok(resp) = lessons.doc(
            catalog,
            language,
            &format!("matins-responsory-{lesson}"),
            diagnostics,
        ) {
            nodes.extend(resp);
        } else if lesson == 9 || (use_abbreviated_sanctoral_lesson && lesson == 3) {
            nodes.extend(formula_nodes(catalog, language, "te-deum", diagnostics)?);
        }
    }
    Ok(nodes)
}

fn resolve_matins_absolution(
    catalog: &Catalog,
    language: &str,
    nocturn: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let mut nodes = formula_nodes(catalog, language, "pater-noster-et", diagnostics)?;
    if let Some(line) = section_lines(catalog, language, BENEDICTIONS, "matins-absolutions")
        .and_then(|lines| lines.get(nocturn.saturating_sub(1)).cloned())
    {
        nodes.push(DocumentNode::Text {
            text: format!("Absolutio. {line}"),
        });
    }
    Ok(nodes)
}

fn resolve_matins_blessing(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    nocturn: usize,
    lesson: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let mut nodes = formula_nodes(catalog, language, "jube-domne", diagnostics)?;
    let section = match nocturn {
        1 => "matins-blessings-nocturn-1",
        2 => "matins-blessings-nocturn-2",
        _ if context.has_rule("lectio1-tempnat") => "matins-blessings-nocturn-3-christmas",
        _ => "matins-blessings-nocturn-3",
    };
    if let Some(line) = section_lines(catalog, language, BENEDICTIONS, section)
        .and_then(|lines| lines.get((lesson - 1) % 3).cloned())
    {
        nodes.push(DocumentNode::Text {
            text: format!("Benedictio. {line}"),
        });
    }
    Ok(nodes)
}

/// The psalmody for an hour. `arg` selects the scheme: `lauds`/`vespers` merge
/// proper antiphons over the ferial day-psalms; `minor` reads a psalter table
/// row (with rubric adjustments); `compline` reads the fixed Compline row.
fn resolve_psalmody(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    arg: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let entries = match arg {
        "lauds" => major_psalmody_entries(catalog, language, context, Hour::Lauds)?,
        "vespers" => major_psalmody_entries(catalog, language, context, Hour::Vespers)?,
        "compline" => {
            let label = weekday_table_label(context.facts.weekday);
            let row = table_row(catalog, language, PSALTER_MINOR, "completorium", label)
                .ok_or_else(|| format!("missing Compline psalmody row `{label}`"))?;
            vec![PsalmodyEntry {
                antiphon: row.text.unwrap_or_default(),
                psalms: row.psalms,
            }]
        }
        // "minor"
        _ => minor_psalmody_entries(catalog, language, context)?,
    };
    let mut nodes = Vec::new();
    for entry in &entries {
        nodes.extend(expand_psalmody_entry(
            catalog,
            language,
            entry,
            diagnostics,
        )?);
    }
    Ok(nodes)
}

fn major_psalmody_entries(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    hour: Hour,
) -> Result<Vec<PsalmodyEntry>, String> {
    let (proper_section, ordinary_section) = match hour {
        Hour::Lauds => (
            "lauds-psalmody",
            format!(
                "day{}-laudes{}",
                if context.has_rule("psalmi-dominica") {
                    0
                } else {
                    divinum_weekday_number(context.facts.weekday)
                },
                context.laudes
            ),
        ),
        Hour::Vespers => (
            "vespers-psalmody",
            format!(
                "day{}-vespera",
                if context.has_rule("psalmi-dominica") {
                    0
                } else {
                    divinum_weekday_number(context.facts.weekday)
                }
            ),
        ),
        _ => return Err("not a major psalmody hour".to_string()),
    };
    let ordinary = section_psalmody(catalog, language, PSALTER_MAJOR, &ordinary_section)
        .ok_or_else(|| format!("missing ordinary psalmody `{ordinary_section}`"))?;
    let principal = context.principal();
    let proper_entries = principal.psalmody(catalog, language, proper_section);
    let proper_antiphons = principal.antiphons(catalog, language, proper_section);
    let entries = if let Some(entries) = proper_entries {
        if entries.iter().any(|entry| !entry.psalms.is_empty()) {
            entries
        } else {
            merge_antiphons_with_psalms(
                entries.into_iter().map(|entry| entry.antiphon).collect(),
                &ordinary,
            )
        }
    } else if let Some(antiphons) = proper_antiphons {
        merge_antiphons_with_psalms(antiphons, &ordinary)
    } else {
        ordinary
    };
    Ok(entries)
}

fn minor_psalmody_entries(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
) -> Result<Vec<PsalmodyEntry>, String> {
    let hour = minor_hour_name(context.hour).ok_or_else(|| "not a minor hour".to_string())?;
    let label = minor_hour_row_label(context.hour, context)
        .ok_or_else(|| "not a minor hour".to_string())?;
    let row = table_row_with_fallbacks(
        catalog,
        language,
        PSALTER_MINOR,
        hour,
        &minor_hour_row_label_candidates(&label),
    )
    .ok_or_else(|| format!("missing minor psalm row `{hour}` `{label}`"))?;
    let mut entry = PsalmodyEntry {
        antiphon: row.text.unwrap_or_default(),
        psalms: row.psalms,
    };
    if context.has_rule("minores-sine-antiphona") {
        entry.antiphon.clear();
    } else if let Some(antiphon) = proper_minor_hour_antiphon(catalog, language, context) {
        entry.antiphon = antiphon;
    }
    if context.omits_optional_psalms() {
        entry.psalms.retain(|psalm| !psalm.optional);
    }
    Ok(vec![entry])
}

/// The chapter block: chapter + hymn + versicle (major hours) or chapter +
/// short responsory + versicle (Prime / minor hours / Compline). `arg` is
/// `major` (Lauds/Vespers) or `minor`; the hour selects the exact shape.
fn resolve_chapter_block(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    arg: &str,
    hour: Hour,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    match (arg, hour) {
        ("major", _) => {
            resolve_major_chapter_hymn_verse(catalog, language, context, hour, diagnostics)
        }
        (_, Hour::Compline) => {
            resolve_compline_chapter_responsory_verse(catalog, language, diagnostics)
        }
        _ => resolve_minor_chapter_responsory_verse(catalog, language, context, diagnostics),
    }
}

fn resolve_major_chapter_hymn_verse(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    hour: Hour,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let is_vespers = hour == Hour::Vespers;
    let hour_word = if is_vespers { "vespers" } else { "lauds" };
    let inherited = context.inherited();
    // Today's seasonal/ferial Ordinary, as books on the stack: the chapter +
    // versicle come from major-{sunday|feria}, the hymn from the season/day book.
    let special = major_special_stack(context, is_vespers);

    // For each part: the proper-book slot candidates, then the canonical slot in
    // the Ordinary stack (which already holds the right season/day book).
    let parts: [(&[&str], &str); 3] = if is_vespers {
        [
            (&["vespers-chapter", "lauds-chapter"], "chapter"),
            (&["vespers-hymn"], "hymn"),
            (&["vespers-versicle", "lauds-versicle"], "versicle"),
        ]
    } else {
        [
            (&["lauds-chapter"], "chapter"),
            (&["lauds-hymn"], "hymn"),
            (&["lauds-versicle"], "versicle"),
        ]
    };

    let mut nodes = Vec::new();
    for (slots, role) in parts {
        let canonical = format!("{hour_word}-{role}");
        let result = inherited
            .of_slots(catalog, language, slots, diagnostics)
            .or_else(|_| special.doc(catalog, language, &canonical, diagnostics))
            .or_else(|error| {
                if role == "hymn" {
                    major_external_hymn_fallback(
                        catalog,
                        language,
                        context,
                        is_vespers,
                        diagnostics,
                    )
                } else {
                    Err(error)
                }
            });
        match result {
            Ok(part) => nodes.extend(part),
            Err(reason) => nodes.push(DocumentNode::Unresolved {
                kind: "section".to_string(),
                value: format!("major {hour_word} {role}"),
                reason,
            }),
        }
    }
    Ok(nodes)
}

/// The seasonal/ferial Ordinary books for the major hours, in priority order:
/// the day's selector (sunday/feria) office for chapter+versicle, then the
/// season's or weekday's hymn book(s) (monastic variant first when seasonal).
fn major_special_stack(context: &OfficeContext, is_vespers: bool) -> Stack {
    let selector = if context.facts.weekday == Weekday::Sun {
        "sunday"
    } else {
        "feria"
    };
    let mut keys = vec![
        format!("ordinary/major-{selector}"),
        format!("ordinary/major-{selector}-2"),
    ];
    match major_seasonal(context) {
        Some(season) => {
            keys.push(format!("ordinary/major-monastic-{season}"));
            keys.push(format!("ordinary/major-{season}"));
        }
        None => {
            let weekday = divinum_weekday_number(context.facts.weekday);
            keys.push(format!("ordinary/major-day{weekday}"));
            if is_vespers && weekday == 6 {
                keys.push("ordinary/major-monastic-day6".to_string());
            }
        }
    }
    Stack::of(keys)
}

/// The seasonal hymn variant for the major hours, if any.
fn major_seasonal(context: &OfficeContext) -> Option<&'static str> {
    let week = &context.facts.temporal_week;
    if week.starts_with("Adv") {
        Some("adv")
    } else if week.starts_with("Quad5") {
        Some("quad5")
    } else if week.starts_with("Quad") {
        Some("quad")
    } else if week.starts_with("Pasc") {
        Some("pasch")
    } else {
        None
    }
}

/// The ferial gospel-canticle antiphon from the major Ordinary, keyed by the
/// day's selector office (`major-dominica-ant` / `major-feria{n}-ant`).
fn ferial_canticle_antiphons(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    slot: &str,
) -> Option<Vec<String>> {
    let weekday = divinum_weekday_number(context.facts.weekday);
    let selector = if weekday == 0 {
        "dominica".to_string()
    } else {
        format!("feria{}", weekday + 1)
    };
    Stack::of([format!("ordinary/major-{selector}-ant")]).antiphons(catalog, language, slot)
}

fn major_external_hymn_fallback(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    is_vespers: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    if !is_vespers || context.facts.weekday != Weekday::Sat {
        return Err("no external hymn fallback".to_string());
    }
    Stack::of([source_key(&["temporal", "Pent01-0"])]).doc(
        catalog,
        language,
        "vespers-hymn",
        diagnostics,
    )
}

/// The gospel canticle for an hour: its antiphon (proper, else the ferial
/// fallback) wrapped around the fixed canticle psalm. `which` is `benedictus`
/// (Lauds), `magnificat` (Vespers), or `nunc-dimittis` (Compline).
fn resolve_canticle(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    which: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let (number, antiphon) = match which {
        "magnificat" => (
            "232",
            context
                .principal()
                .antiphons(catalog, language, "vespers-gospel-antiphon")
                .or_else(|| {
                    ferial_canticle_antiphons(catalog, language, context, "vespers-gospel-antiphon")
                }),
        ),
        "nunc-dimittis" => {
            let compline = Stack::of(["ordinary/compline"]);
            (
                "233",
                compline
                    .antiphons(catalog, language, compline_antiphon_section(context))
                    .or_else(|| compline.antiphons(catalog, language, "gospel-antiphon")),
            )
        }
        _ => (
            "231",
            context
                .principal()
                .antiphons(catalog, language, "lauds-gospel-antiphon")
                .or_else(|| {
                    ferial_canticle_antiphons(catalog, language, context, "lauds-gospel-antiphon")
                }),
        ),
    };
    let antiphon = antiphon
        .and_then(|values| values.into_iter().next())
        .unwrap_or_default();
    gospel_canticle_nodes(catalog, language, number, &antiphon, diagnostics)
}

fn resolve_minor_chapter_responsory_verse(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    if context.hour == Hour::Prime {
        let selector =
            if context.facts.weekday == Weekday::Sun || context.has_rule("psalmi-dominica") {
                "sunday"
            } else {
                "feria"
            };
        let fixed = Stack::of(["ordinary/prime-fixed"]);
        let season = Stack::of([format!("ordinary/prime-{}", prime_season(context))]);
        let mut nodes = Stack::of([format!("ordinary/prime-{selector}")]).doc(
            catalog,
            language,
            "chapter",
            diagnostics,
        )?;
        nodes.extend(fixed.doc(catalog, language, "short-responsory", diagnostics)?);
        if let Ok(seasonal) = season.doc(catalog, language, "seasonal-responsory", diagnostics) {
            nodes.extend(seasonal);
        }
        nodes.extend(fixed.doc(catalog, language, "versicle", diagnostics)?);
        return Ok(nodes);
    }

    let canonical_hour =
        canonical_minor_hour(context.hour).ok_or_else(|| "not a minor hour".to_string())?;
    let principal = context.principal();
    // Today's seasonal Ordinary for the minor hours, keyed by canonical slots.
    let special = Stack::of([format!("ordinary/minor-{}", minor_special_season(context))]);

    let chapter_slots: Vec<String> = if context.hour == Hour::Terce {
        vec![
            format!("{canonical_hour}-chapter"),
            "lauds-chapter".to_string(),
        ]
    } else {
        vec![format!("{canonical_hour}-chapter")]
    };
    let chapter_refs: Vec<&str> = chapter_slots.iter().map(String::as_str).collect();
    let mut nodes = principal
        .of_slots(catalog, language, &chapter_refs, diagnostics)
        .or_else(|_| {
            special.doc(
                catalog,
                language,
                &format!("{canonical_hour}-chapter"),
                diagnostics,
            )
        })
        .map_err(|_| format!("missing minor chapter for {canonical_hour}"))?;

    for (role, optional) in [("short-responsory", false), ("versicle", true)] {
        let slot = format!("{canonical_hour}-{role}");
        match principal
            .doc(catalog, language, &slot, diagnostics)
            .or_else(|_| special.doc(catalog, language, &slot, diagnostics))
        {
            Ok(part) => nodes.extend(part),
            Err(_) if optional => {}
            Err(error) => return Err(error),
        }
    }
    Ok(nodes)
}

fn resolve_prime_martyrology(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let next_day = context.facts.date + Duration::days(1);
    let key = sanctoral_key(next_day);
    let source = source_key(&["martyrology", &key]);
    match Stack::of([source]).doc(catalog, language, "raw", diagnostics) {
        Ok(nodes) => Ok(nodes),
        Err(reason) => Ok(vec![DocumentNode::Unresolved {
            kind: "section".to_string(),
            value: format!("martyrology {key}"),
            reason,
        }]),
    }
}

fn resolve_prime_short_reading(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let mut nodes = formula_nodes(catalog, language, "jube-domne", diagnostics)?;
    nodes.extend(formula_nodes(
        catalog,
        language,
        "benedictio-prima",
        diagnostics,
    )?);
    nodes.extend(
        context
            .principal()
            .doc(catalog, language, "prime-short-reading", diagnostics)
            .or_else(|_| {
                Stack::of([format!("ordinary/prime-{}", prime_season(context))]).doc(
                    catalog,
                    language,
                    "short-reading",
                    diagnostics,
                )
            })?,
    );
    Ok(nodes)
}

fn resolve_compline_chapter_responsory_verse(
    catalog: &Catalog,
    language: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let compline = Stack::of(["ordinary/compline"]);
    let mut nodes = compline.doc(catalog, language, "chapter", diagnostics)?;
    nodes.extend(compline.doc(catalog, language, "short-responsory", diagnostics)?);
    nodes.extend(compline.doc(catalog, language, "versicle", diagnostics)?);
    Ok(nodes)
}

/// The collect block. Prime and Compline use a fixed ordinary collect; every
/// other hour resolves the proper collect from the stack and appends any
/// commemorations. `arg` is `prime`/`compline`/`standard`/`daytime`.
fn resolve_collect(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    arg: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let mut nodes = domine_exaudi_nodes(language);
    nodes.extend(formula_nodes(catalog, language, "oremus", diagnostics)?);
    match arg {
        "prime" => {
            nodes.extend(formula_nodes(
                catalog,
                language,
                "oratio-domine",
                diagnostics,
            )?);
            nodes.extend(formula_nodes(
                catalog,
                language,
                "per-dominum",
                diagnostics,
            )?);
            nodes.extend(domine_exaudi_nodes(language));
            nodes.extend(formula_nodes(
                catalog,
                language,
                "benedicamus-domino",
                diagnostics,
            )?);
        }
        "compline" => {
            nodes.extend(formula_nodes(
                catalog,
                language,
                "oratio-visita",
                diagnostics,
            )?);
            nodes.extend(formula_nodes(
                catalog,
                language,
                "per-dominum",
                diagnostics,
            )?);
        }
        _ => {
            match first_collect_doc(catalog, language, context, &context.collect(), diagnostics) {
                Ok(oratio) => nodes.extend(oratio),
                Err(reason) => nodes.push(DocumentNode::Unresolved {
                    kind: "section".to_string(),
                    value: "collect".to_string(),
                    reason,
                }),
            }
            for commemoration in &context.commemorations {
                nodes.extend(resolve_commemoration(
                    catalog,
                    language,
                    context,
                    commemoration,
                    diagnostics,
                )?);
            }
        }
    }
    Ok(nodes)
}

fn resolve_commemoration(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    commemoration: &CommemorationContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let mut nodes = Vec::new();
    let sources = commemoration_sources(context, commemoration);
    if let Some(title) = section_lines(catalog, language, &commemoration.source_key, "title")
        .and_then(|lines| lines.into_iter().next())
    {
        nodes.push(DocumentNode::Marker {
            text: format!(
                "{} {}",
                localized_literal(language, "Commemoratio", "Commemoration of"),
                title
            ),
        });
    }
    let indexed_antiphon = if context.hour == Hour::Vespers {
        "vespers-gospel-antiphon"
    } else {
        "lauds-gospel-antiphon"
    };
    let indexed_versicle = if context.hour == Hour::Vespers {
        "vespers-versicle"
    } else {
        "lauds-versicle"
    };
    if let Some(antiphon) = sources
        .antiphons(catalog, language, indexed_antiphon)
        .and_then(|mut values| values.pop())
    {
        nodes.push(DocumentNode::Text {
            text: format!("Ant. {}", close_antiphon(&antiphon)),
        });
    }
    if let Ok(versicle) = sources.doc(catalog, language, indexed_versicle, diagnostics) {
        nodes.extend(versicle);
    }
    nodes.extend(formula_nodes(catalog, language, "oremus", diagnostics)?);
    match first_collect_doc(catalog, language, context, &sources, diagnostics) {
        Ok(oratio) => nodes.extend(oratio),
        Err(reason) => nodes.push(DocumentNode::Unresolved {
            kind: "section".to_string(),
            value: "collect".to_string(),
            reason,
        }),
    }
    Ok(nodes)
}

fn first_collect_doc(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    sources: &Stack,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    // Source-major order (try every collect section within one book before the
    // next book), unlike the slot-major `of_slots`.
    let sections = collect_section_candidates(context.hour);
    for source in &sources.keys {
        for section in &sections {
            if let Some(nodes) = section_nodes(catalog, language, source, section) {
                return expand_nodes(catalog, language, &nodes, diagnostics);
            }
        }
    }
    Err(format!("missing sections {sections:?}"))
}

fn collect_section_candidates(hour: Hour) -> Vec<&'static str> {
    match hour {
        Hour::Vespers => vec!["vespers-collect", "collect", "daytime-collect"],
        Hour::Matins => vec!["matins-collect", "collect"],
        _ => vec!["daytime-collect", "collect"],
    }
}

fn resolve_final_antiphon(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let mut nodes = Stack::of(["ordinary/marian-antiphons"]).doc(
        catalog,
        language,
        final_antiphon_section(context),
        diagnostics,
    )?;
    if context.hour == Hour::Compline {
        nodes.extend(divinum_auxilium_nodes(language));
    }
    Ok(nodes)
}

fn formula_nodes(
    catalog: &Catalog,
    language: &str,
    section: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let source = source_key(&["ordinary", "formulae"]);
    if let Some(nodes) = section_nodes(catalog, language, &source, section) {
        return expand_nodes(catalog, language, &nodes, diagnostics);
    }
    Err(format!("missing formula `{section}` in `{language}`"))
}

fn first_formula_doc(
    catalog: &Catalog,
    language: &str,
    sections: &[&str],
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    for section in sections {
        if let Ok(nodes) = formula_nodes(catalog, language, section, diagnostics) {
            return Ok(nodes);
        }
    }
    Err(format!("missing formulas {sections:?} in `{language}`"))
}

fn amen_nodes() -> Vec<DocumentNode> {
    vec![DocumentNode::Amen]
}

fn domine_exaudi_nodes(language: &str) -> Vec<DocumentNode> {
    vec![
        DocumentNode::Versicle {
            text: localized_literal(
                language,
                "Dómine, exáudi oratiónem meam.",
                "O Lord, hear my prayer.",
            )
            .to_string(),
        },
        DocumentNode::Response {
            text: localized_literal(
                language,
                "Et clamor meus ad te véniat.",
                "And let my cry come unto thee.",
            )
            .to_string(),
        },
    ]
}

fn divinum_auxilium_nodes(language: &str) -> Vec<DocumentNode> {
    vec![
        DocumentNode::Versicle {
            text: localized_literal(
                language,
                "Divínum auxílium ✠ máneat semper nobíscum.",
                "May the divine assistance ✠ remain with us always.",
            )
            .to_string(),
        },
        DocumentNode::Amen,
    ]
}

fn formula_lines(
    catalog: &Catalog,
    language: &str,
    section: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<String>, String> {
    formula_nodes(catalog, language, section, diagnostics).map(|nodes| document_lines(&nodes))
}

fn expand_nodes(
    catalog: &Catalog,
    language: &str,
    nodes: &[ContentNode],
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let mut output = Vec::new();
    for node in nodes {
        match node {
            // The importer already typed and cleaned every text node, so render
            // is a 1:1 map. The only non-trivial cases are reference expansion
            // (psalms) and the `Amen` response classification.
            ContentNode::Text { text } => output.push(DocumentNode::Text { text: text.clone() }),
            ContentNode::Rubric { text } => {
                output.push(DocumentNode::Rubric { text: text.clone() })
            }
            ContentNode::Marker { text } => {
                output.push(DocumentNode::Marker { text: text.clone() })
            }
            ContentNode::Heading { text } => {
                output.push(DocumentNode::Heading { text: text.clone() })
            }
            ContentNode::Citation { text } => {
                output.push(DocumentNode::Citation { text: text.clone() })
            }
            ContentNode::Versicle { text } => {
                output.push(DocumentNode::Versicle { text: text.clone() })
            }
            ContentNode::Response { text } => {
                if is_amen_text(text) {
                    output.push(DocumentNode::Amen);
                } else {
                    output.push(DocumentNode::Response { text: text.clone() });
                }
            }
            ContentNode::ShortResponse { text } => {
                output.push(DocumentNode::ShortResponse { text: text.clone() })
            }
            ContentNode::Antiphon { text } => {
                output.push(DocumentNode::Antiphon { text: text.clone() })
            }
            ContentNode::Prayer { text } => {
                output.push(DocumentNode::Prayer { text: text.clone() })
            }
            ContentNode::Blessing { text } => {
                output.push(DocumentNode::Blessing { text: text.clone() })
            }
            ContentNode::PsalmRef {
                number,
                start,
                end,
                optional,
            } => output.extend(psalm_nodes(
                catalog,
                language,
                &PsalmReference {
                    number: number.clone(),
                    start: start.clone(),
                    end: end.clone(),
                    optional: *optional,
                },
                diagnostics,
            )?),
            ContentNode::Psalmody { antiphon, psalms } => output.extend(expand_psalmody_entry(
                catalog,
                language,
                &PsalmodyEntry {
                    antiphon: antiphon.clone(),
                    psalms: psalms.clone(),
                },
                diagnostics,
            )?),
            ContentNode::TableRow {
                label,
                text,
                psalms,
            } => {
                output.push(DocumentNode::Heading {
                    text: label.clone(),
                });
                output.extend(expand_psalmody_entry(
                    catalog,
                    language,
                    &PsalmodyEntry {
                        antiphon: text.clone().unwrap_or_default(),
                        psalms: psalms.clone(),
                    },
                    diagnostics,
                )?);
            }
            ContentNode::Rank { .. } | ContentNode::Rule { .. } => {}
        }
    }
    Ok(output)
}

fn is_amen_text(text: &str) -> bool {
    text.trim()
        .trim_end_matches('.')
        .eq_ignore_ascii_case("amen")
}

#[derive(Clone, Debug)]
struct PsalmodyEntry {
    antiphon: String,
    psalms: Vec<PsalmReference>,
}

#[derive(Clone, Debug)]
struct TableEntry {
    text: Option<String>,
    psalms: Vec<PsalmReference>,
}

fn expand_psalmody_entry(
    catalog: &Catalog,
    language: &str,
    entry: &PsalmodyEntry,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let mut nodes = Vec::new();
    // The antiphon was cleaned at import; only the closing repeat is shortened.
    if !entry.antiphon.is_empty() {
        nodes.push(DocumentNode::Antiphon {
            text: entry.antiphon.clone(),
        });
    }
    for psalm in &entry.psalms {
        match psalm_nodes(catalog, language, psalm, diagnostics) {
            Ok(psalm_nodes) => nodes.extend(psalm_nodes),
            Err(reason) => nodes.push(DocumentNode::Unresolved {
                kind: "psalm".to_string(),
                value: localized_psalm_title(language, psalm),
                reason,
            }),
        }
    }
    if !entry.antiphon.is_empty() {
        nodes.push(DocumentNode::Antiphon {
            text: close_antiphon(&entry.antiphon),
        });
    }
    Ok(nodes)
}

fn psalm_nodes(
    catalog: &Catalog,
    language: &str,
    reference: &PsalmReference,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let source = source_key(&["psalm", &reference.number]);
    let raw = section_nodes(catalog, language, &source, "raw")
        .ok_or_else(|| format!("missing psalm {}", reference.number))?;
    let mut lines =
        expand_nodes(catalog, language, &raw, diagnostics).map(|nodes| document_lines(&nodes))?;
    if let Some(start) = &reference.start {
        let start = parse_verse_ref(start)?;
        let end = reference
            .end
            .as_deref()
            .map(parse_verse_ref)
            .transpose()?
            .unwrap_or(start);
        lines.retain(|line| {
            verse_ref_from_line(&reference.number, line)
                .is_some_and(|verse| verse >= start && verse <= end)
        });
    }
    let mut nodes = Vec::new();
    if !is_gospel_canticle_number(&reference.number) {
        nodes.push(DocumentNode::Heading {
            text: localized_psalm_title(language, reference),
        });
    }
    if !lines.is_empty() {
        nodes.push(DocumentNode::Text {
            text: lines.join("\n"),
        });
    }
    if reference.number != "210" {
        nodes.extend(formula_nodes(catalog, language, "gloria", diagnostics).unwrap_or_default());
    }
    Ok(nodes)
}

fn is_gospel_canticle_number(number: &str) -> bool {
    matches!(number, "231" | "232" | "233")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct VerseRef {
    number: u16,
    suffix: u8,
}

fn parse_verse_ref(input: &str) -> Result<VerseRef, String> {
    let input = input.trim().trim_matches('\'');
    let digits = input
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    let number = digits
        .parse::<u16>()
        .map_err(|_| format!("malformed verse `{input}`"))?;
    let suffix = input[digits.len()..].chars().next().map_or(0, |ch| {
        if ch.is_ascii_alphabetic() {
            ch.to_ascii_lowercase() as u8 - b'a' + 1
        } else {
            0
        }
    });
    Ok(VerseRef { number, suffix })
}

fn verse_ref_from_line(psalm_number: &str, line: &str) -> Option<VerseRef> {
    let rest = line
        .trim_start()
        .strip_prefix(psalm_number)?
        .strip_prefix(':')?;
    let digits = rest
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        return None;
    }
    let suffix = rest[digits.len()..].chars().next().map_or(0, |ch| {
        if ch.is_ascii_alphabetic() {
            ch.to_ascii_lowercase() as u8 - b'a' + 1
        } else {
            0
        }
    });
    digits
        .parse::<u16>()
        .ok()
        .map(|number| VerseRef { number, suffix })
}

fn section_lines(
    catalog: &Catalog,
    language: &str,
    source_key: &str,
    section: &str,
) -> Option<Vec<String>> {
    section_nodes(catalog, language, source_key, section).map(|nodes| content_lines(&nodes))
}

fn section_antiphons(
    catalog: &Catalog,
    language: &str,
    source_key: &str,
    section: &str,
) -> Option<Vec<String>> {
    let nodes = section_nodes(catalog, language, source_key, section)?;
    let mut values = Vec::new();
    for node in nodes {
        match node {
            ContentNode::Antiphon { text } => values.push(text),
            ContentNode::Psalmody { antiphon, .. } if !antiphon.is_empty() => values.push(antiphon),
            ContentNode::Text { text } => values.extend(
                text.lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(ToOwned::to_owned),
            ),
            _ => {}
        }
    }
    (!values.is_empty()).then_some(values)
}

fn section_psalmody(
    catalog: &Catalog,
    language: &str,
    source_key: &str,
    section: &str,
) -> Option<Vec<PsalmodyEntry>> {
    let nodes = section_nodes(catalog, language, source_key, section)?;
    let mut entries = Vec::new();
    for node in nodes {
        match node {
            ContentNode::Psalmody { antiphon, psalms } => {
                entries.push(PsalmodyEntry { antiphon, psalms });
            }
            ContentNode::TableRow { text, psalms, .. } => entries.push(PsalmodyEntry {
                antiphon: text.unwrap_or_default(),
                psalms,
            }),
            _ => {}
        }
    }
    (!entries.is_empty()).then_some(entries)
}

fn table_row(
    catalog: &Catalog,
    language: &str,
    source_key: &str,
    section: &str,
    label: &str,
) -> Option<TableEntry> {
    let nodes = section_nodes(catalog, language, source_key, section)?;
    let label = normalize_space(label);
    nodes.into_iter().find_map(|node| match node {
        ContentNode::TableRow {
            label: row_label,
            text,
            psalms,
        } if normalize_space(&row_label) == label => Some(TableEntry { text, psalms }),
        _ => None,
    })
}

fn table_row_with_fallbacks(
    catalog: &Catalog,
    language: &str,
    source_key: &str,
    section: &str,
    labels: &[String],
) -> Option<TableEntry> {
    labels
        .iter()
        .find_map(|label| table_row(catalog, language, source_key, section, label))
}

fn observance_from_source_key(
    catalog: &Catalog,
    language: &str,
    source_key: &str,
    kind: ObservanceKind,
    id: RecordId,
) -> OfficeObservance {
    let title = section_lines(catalog, language, source_key, "title")
        .and_then(|lines| lines.into_iter().next());
    let office = catalog.office(source_key);
    OfficeObservance {
        id,
        title,
        kind,
        rank: office.and_then(|office| office.rank),
        rank_label: office.and_then(|office| office.rank_name.clone()),
        catalog_key: Some(source_key.to_string()),
    }
}

fn choose_principal(
    temporal: Option<&OfficeObservance>,
    sanctoral: Option<&OfficeObservance>,
    facts: &DateFacts,
    trace: &mut Vec<TraceEvent>,
) -> Option<OfficeObservance> {
    let selected = match (temporal, sanctoral) {
        (Some(temporal), Some(sanctoral)) => {
            let temporal_rank = temporal.rank.unwrap_or(1.0);
            let sanctoral_rank = sanctoral.rank.unwrap_or(0.0);
            if sanctoral_rank < 2.0 {
                temporal
            } else if sanctoral_rank > temporal_rank
                && !(facts.weekday == Weekday::Sun && temporal_rank >= 5.0 && sanctoral_rank < 6.0)
            {
                sanctoral
            } else {
                temporal
            }
        }
        (Some(temporal), None) => temporal,
        (None, Some(sanctoral)) => sanctoral,
        (None, None) => return None,
    };
    trace.push(TraceEvent {
        phase: "precedence",
        message: format!("selected `{}` rank {:?}", selected.id, selected.rank),
    });
    Some(selected.clone())
}

fn non_principal_candidate<'a>(
    principal: &OfficeObservance,
    temporal: Option<&'a OfficeObservance>,
    sanctoral: Option<&'a OfficeObservance>,
) -> Option<&'a OfficeObservance> {
    temporal
        .into_iter()
        .chain(sanctoral)
        .find(|candidate| candidate.id != principal.id)
}

fn merge_antiphons_with_psalms(
    antiphons: Vec<String>,
    ordinary: &[PsalmodyEntry],
) -> Vec<PsalmodyEntry> {
    antiphons
        .into_iter()
        .enumerate()
        .filter_map(|(index, antiphon)| {
            ordinary.get(index).map(|entry| PsalmodyEntry {
                antiphon,
                psalms: entry.psalms.clone(),
            })
        })
        .collect()
}

fn gospel_canticle_nodes(
    catalog: &Catalog,
    language: &str,
    number: &str,
    antiphon: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    expand_psalmody_entry(
        catalog,
        language,
        &PsalmodyEntry {
            antiphon: antiphon.to_string(),
            psalms: vec![PsalmReference {
                number: number.to_string(),
                start: None,
                end: None,
                optional: false,
            }],
        },
        diagnostics,
    )
}

fn commemoration_sources(context: &OfficeContext, commemoration: &CommemorationContext) -> Stack {
    let mut keys: Vec<String> = vec![commemoration.source_key.clone()];
    keys.extend(commemoration.commune_key.clone());
    let temporal = context
        .temporal_key
        .as_deref()
        .is_some_and(|key| key == commemoration.source_key)
        || source_key_category(&commemoration.source_key) == Some("temporal");
    if temporal {
        keys.extend(context.weekly_temporal_key.clone());
        keys.extend(
            context
                .collect_reference_keys
                .iter()
                .filter(|key| source_key_category(key) == Some("temporal"))
                .cloned(),
        );
    }
    Stack::of(keys)
}

fn document_lines(nodes: &[DocumentNode]) -> Vec<String> {
    nodes
        .iter()
        .flat_map(|node| {
            node.plain_text()
                .lines()
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .collect()
}

fn content_lines(nodes: &[ContentNode]) -> Vec<String> {
    let mut lines = Vec::new();
    for node in nodes {
        match node {
            ContentNode::Text { text }
            | ContentNode::Rubric { text }
            | ContentNode::Marker { text }
            | ContentNode::Heading { text }
            | ContentNode::Citation { text }
            | ContentNode::Versicle { text }
            | ContentNode::Response { text }
            | ContentNode::ShortResponse { text }
            | ContentNode::Antiphon { text }
            | ContentNode::Prayer { text }
            | ContentNode::Blessing { text } => {
                lines.extend(text.lines().map(ToOwned::to_owned));
            }
            ContentNode::Psalmody { antiphon, .. } => lines.push(antiphon.clone()),
            ContentNode::TableRow { label, text, .. } => {
                lines.push(format!("{} {}", label, text.as_deref().unwrap_or("")));
            }
            ContentNode::PsalmRef {
                number, start, end, ..
            } => lines.push(psalm_label(number, start.as_deref(), end.as_deref())),
            ContentNode::Rank {
                label,
                value,
                common,
            } => lines.push(format!(
                "{} {} {}",
                label.as_deref().unwrap_or(""),
                value.as_ref().map(ToString::to_string).unwrap_or_default(),
                common.as_deref().unwrap_or("")
            )),
            ContentNode::Rule { tokens } => {
                lines.extend(tokens.iter().map(|token| token.label().to_string()))
            }
        }
    }
    lines
}

fn source_key(parts: &[&str]) -> String {
    parts
        .iter()
        .map(|part| data_slug(part))
        .collect::<Vec<_>>()
        .join("/")
}

fn collect_reference_keys_for_sources(
    catalog: &Catalog,
    primary_language: &str,
    sources: &[Option<&str>],
) -> Vec<String> {
    let mut keys = Vec::new();
    let mut seen = BTreeSet::new();
    for source in sources.iter().flatten() {
        push_collect_reference_key(catalog, primary_language, source, &mut keys, &mut seen);
    }
    keys
}

fn push_collect_reference_key(
    catalog: &Catalog,
    primary_language: &str,
    source_key: &str,
    keys: &mut Vec<String>,
    seen: &mut BTreeSet<String>,
) {
    if !seen.insert(source_key.to_string()) {
        return;
    }
    // `office.common` is already a canonical `book/office` key.
    let Some(key) = catalog.office(source_key).and_then(|o| o.common.clone()) else {
        return;
    };
    if !keys.iter().any(|existing| existing == &key) {
        keys.push(key.clone());
    }
    push_collect_reference_key(catalog, primary_language, &key, keys, seen);
}

fn source_key_category(source_key: &str) -> Option<&str> {
    source_key.split('/').next()
}

fn first_existing_source_key(
    catalog: &Catalog,
    language: &str,
    candidates: Vec<String>,
) -> Option<String> {
    let _ = language;
    candidates
        .into_iter()
        .find(|candidate| catalog.has_office(candidate))
}

fn temporal_source_candidates(facts: &DateFacts) -> Vec<String> {
    vec![source_key(&["temporal", &facts.temporal_stem])]
}

fn sanctoral_source_candidates(_catalog: &Catalog, _language: &str, date_key: &str) -> Vec<String> {
    vec![source_key(&["sanctoral", date_key])]
}

fn weekly_temporal_source_candidates(facts: &DateFacts) -> Vec<String> {
    if facts.temporal_week.starts_with("Nat") {
        return Vec::new();
    }
    vec![source_key(&[
        "temporal",
        &format!("{}-0", facts.temporal_week),
    ])]
}

fn adjacent_temporal_key(catalog: &Catalog, language: &str, date: NaiveDate) -> Option<String> {
    let facts = office_date_facts(date).ok()?;
    first_existing_source_key(catalog, language, temporal_source_candidates(&facts))
}

fn scripture_source_candidates(facts: &DateFacts) -> Vec<String> {
    let weekday = i64::from(divinum_weekday_number(facts.weekday));
    let week_start = facts.date - Duration::days(weekday);
    let month = week_start.month();
    if !(8..=11).contains(&month) {
        return Vec::new();
    }
    let first_of_month =
        NaiveDate::from_ymd_opt(week_start.year(), month, 1).expect("valid month start");
    let days_to_first_sunday = (7 - divinum_weekday_number(first_of_month.weekday())).rem_euclid(7);
    let first_sunday = first_of_month + Duration::days(i64::from(days_to_first_sunday));
    if week_start < first_sunday {
        return Vec::new();
    }
    let week = 1 + ((week_start - first_sunday).num_days() / 7);
    let weekday = divinum_weekday_number(facts.weekday);
    let mut stems = vec![format!("{month:02}{week}-{weekday}")];
    if weekday != 0 {
        stems.push(format!("{month:02}{week}-0"));
    }
    stems
        .into_iter()
        .map(|stem| source_key(&["temporal", &stem]))
        .collect()
}

fn localized_literal<'a>(language: &str, latin: &'a str, english: &'a str) -> &'a str {
    if language == "en" {
        english
    } else {
        latin
    }
}

fn localized_nocturn_title(language: &str, nocturn: usize) -> String {
    if language == "en" {
        format!("Nocturn {nocturn}")
    } else {
        format!("Nocturnus {nocturn}")
    }
}

fn localized_lesson_title(language: &str, lesson: usize) -> String {
    if language == "en" {
        format!("Reading {lesson}")
    } else {
        format!("Lectio {lesson}")
    }
}

fn localized_psalm_title(language: &str, psalm: &PsalmReference) -> String {
    let label = psalm_label(&psalm.number, psalm.start.as_deref(), psalm.end.as_deref());
    if language == "en" {
        format!("Psalm {label}")
    } else {
        format!("Psalmus {label}")
    }
}

fn psalm_label(number: &str, start: Option<&str>, end: Option<&str>) -> String {
    let mut label = number.to_string();
    if let Some(start) = start {
        label.push('(');
        label.push_str(start);
        if let Some(end) = end {
            label.push('-');
            label.push_str(end);
        }
        label.push(')');
    }
    label
}

fn close_antiphon(antiphon: &str) -> String {
    antiphon
        .replace(" * ", " ")
        .replace('*', "")
        .trim()
        .to_string()
}

fn normalize_rule_id(input: &str) -> String {
    data_slug(input)
}

fn normalize_space(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}
fn weekday_table_label(weekday: Weekday) -> &'static str {
    match weekday {
        Weekday::Sun => "sunday",
        Weekday::Mon => "monday",
        Weekday::Tue => "tuesday",
        Weekday::Wed => "wednesday",
        Weekday::Thu => "thursday",
        Weekday::Fri => "friday",
        Weekday::Sat => "saturday",
    }
}

fn minor_hour_name(hour: Hour) -> Option<&'static str> {
    match hour {
        Hour::Prime => Some("prima"),
        Hour::Terce => Some("tertia"),
        Hour::Sext => Some("sexta"),
        Hour::None => Some("nona"),
        _ => None,
    }
}

fn canonical_minor_hour(hour: Hour) -> Option<&'static str> {
    match hour {
        Hour::Prime => Some("prime"),
        Hour::Terce => Some("terce"),
        Hour::Sext => Some("sext"),
        Hour::None => Some("none"),
        _ => None,
    }
}

fn minor_hour_antiphon_index(hour: Hour) -> Option<usize> {
    match hour {
        Hour::Prime => Some(0),
        Hour::Terce => Some(1),
        Hour::Sext => Some(2),
        Hour::None => Some(4),
        _ => None,
    }
}

fn minor_hour_row_label(hour: Hour, context: &OfficeContext) -> Option<String> {
    if hour == Hour::Prime {
        return Some(weekday_table_label(context.facts.weekday).to_string());
    }
    if context.facts.weekday == Weekday::Sun || context.has_rule("psalmi-dominica") {
        Some("sunday".to_string())
    } else {
        Some(weekday_table_label(context.facts.weekday).to_string())
    }
}

fn minor_hour_row_label_candidates(label: &str) -> Vec<String> {
    vec![label.to_string()]
}

fn proper_minor_hour_antiphon(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
) -> Option<String> {
    let sources = context.principal();
    if let Some(antiphon) = canonical_minor_hour(context.hour)
        .and_then(|canonical_hour| {
            sources.antiphons(catalog, language, &format!("{canonical_hour}-antiphon"))
        })
        .and_then(first_nonempty_antiphon)
    {
        return Some(antiphon);
    }
    if !context.has_rule("antiphonas-horas") {
        return None;
    }
    let index = minor_hour_antiphon_index(context.hour)?;
    ["lauds-psalmody", "vespers-psalmody"]
        .into_iter()
        .find_map(|section| {
            sources
                .antiphons(catalog, language, section)
                .and_then(|values| values.get(index).cloned())
                .filter(|value| !value.trim().is_empty())
        })
}

fn first_nonempty_antiphon(values: Vec<String>) -> Option<String> {
    values.into_iter().find(|value| !value.trim().is_empty())
}

fn minor_special_season(context: &OfficeContext) -> &'static str {
    if context.facts.temporal_week.starts_with("Adv") {
        "adv"
    } else if context.facts.temporal_week.starts_with("Quad5") {
        "quad5"
    } else if context.facts.temporal_week.starts_with("Quad") {
        "quad"
    } else if context.facts.temporal_week.starts_with("Pasc") {
        "pasch"
    } else if context.facts.weekday == Weekday::Sun {
        "dominica"
    } else {
        "feria"
    }
}

fn prime_season(context: &OfficeContext) -> &'static str {
    if context.facts.temporal_week.starts_with("Adv") {
        "adv"
    } else if context.facts.temporal_week.starts_with("Nat") {
        "nat"
    } else if context.facts.temporal_week.starts_with("Epi") {
        "epi"
    } else if context.facts.temporal_week.starts_with("Quad5") {
        "quad5"
    } else if context.facts.temporal_week.starts_with("Quad") {
        "quad"
    } else if context.facts.temporal_week.starts_with("Pasc") {
        "pasch"
    } else {
        "per-annum"
    }
}

fn matins_ordinary_hymn_section(context: &OfficeContext) -> String {
    if context.facts.temporal_week.starts_with("Adv") {
        "hymnus-adv".to_string()
    } else if context.facts.temporal_week.starts_with("Quad") {
        "hymnus-quad".to_string()
    } else if context.facts.temporal_week.starts_with("Pasc") {
        "hymnus-pasch".to_string()
    } else {
        format!(
            "day{}-hymnus",
            divinum_weekday_number(context.facts.weekday)
        )
    }
}

fn compline_antiphon_section(context: &OfficeContext) -> &'static str {
    if context.facts.temporal_week.starts_with("Quad5") {
        "gospel-antiphon-passiontide"
    } else if context.facts.temporal_week.starts_with("Quad") {
        "gospel-antiphon-lent"
    } else if context.facts.temporal_week.starts_with("Pasc") {
        "gospel-antiphon-easter"
    } else {
        "gospel-antiphon"
    }
}

fn final_antiphon_section(context: &OfficeContext) -> &'static str {
    let date = context.facts.date;
    if context.facts.temporal_week.starts_with("Adv") {
        "advent"
    } else if date.month() == 12 && date.day() >= 25 || date.month() == 1 {
        "nativiti"
    } else if date >= context.facts.easter && date <= context.facts.easter + Duration::days(56) {
        "paschalis"
    } else if context.facts.temporal_week.starts_with("Quad") {
        "quadragesimae"
    } else {
        "postpentecost"
    }
}
