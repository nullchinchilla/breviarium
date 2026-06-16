#![deny(missing_docs)]
//! Embedded structured liturgical data and Office resolution.
//!
//! `breviarium-data` embeds YAML data at compile time and exposes a typed API
//! for resolving Office hours. The YAML format is normalized into two layers:
//! reusable multilingual corpus texts, and liturgical source sections that
//! reference those corpus texts by ID. Divinum Officium source syntax is
//! consumed by the importer; runtime code sees ordinary records such as
//! antiphons, psalm references, rank metadata, and rule tokens.
//!
//! # Example
//!
//! ```
//! use breviarium_data::{Breviarium, Hour, OfficeRequest};
//! use chrono::NaiveDate;
//!
//! let engine = Breviarium::embedded().unwrap();
//! let request = OfficeRequest::new(
//!     NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
//!     Hour::Lauds,
//! );
//! let office = engine.resolve_office(request).unwrap();
//! assert_eq!(office.hour, Hour::Lauds);
//! assert!(!office.blocks.is_empty());
//! ```

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use include_dir::{include_dir, Dir};
use ordered_float::OrderedFloat;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;
use thiserror::Error;

static DATA_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/data");
static CATALOG: OnceLock<Result<Catalog, DataError>> = OnceLock::new();

/// Stable identifier for a rubrical profile such as `roman-1960`.
pub type ProfileId = String;

/// Stable identifier for a language such as `la` or `en`.
pub type LanguageId = String;

/// Stable identifier for catalog records and output blocks.
pub type RecordId = String;

/// Canonical Office hour.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Hour {
    /// Matins.
    Matins,
    /// Lauds.
    Lauds,
    /// Prime.
    Prime,
    /// Terce.
    Terce,
    /// Sext.
    Sext,
    /// None.
    None,
    /// Vespers.
    Vespers,
    /// Compline.
    Compline,
}

impl Hour {
    /// Stable lowercase identifier.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Matins => "matins",
            Self::Lauds => "lauds",
            Self::Prime => "prime",
            Self::Terce => "terce",
            Self::Sext => "sext",
            Self::None => "none",
            Self::Vespers => "vespers",
            Self::Compline => "compline",
        }
    }
}

/// High-level kind of observance.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ObservanceKind {
    /// Temporal cycle observance.
    Temporal,
    /// Fixed sanctoral observance.
    Sanctoral,
    /// Common text source.
    Common,
    /// Votive observance.
    Votive,
}

/// Semantic role of a text section or output block.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Ord, PartialOrd)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum TextRole {
    /// Opening formulae.
    Opening,
    /// Invitatory.
    Invitatory,
    /// Hymn.
    Hymn,
    /// Antiphon.
    Antiphon,
    /// Psalm.
    Psalm,
    /// Canticle.
    Canticle,
    /// Psalmody.
    Psalmody,
    /// Lesson or reading.
    Reading,
    /// Short reading.
    ShortReading,
    /// Responsory.
    Responsory,
    /// Short responsory.
    ShortResponsory,
    /// Versicle.
    Versicle,
    /// Absolution.
    Absolution,
    /// Blessing.
    Blessing,
    /// Chapter.
    Chapter,
    /// Gospel canticle.
    GospelCanticle,
    /// Preces.
    Preces,
    /// Collect.
    Collect,
    /// Commemoration antiphon.
    CommemorationAntiphon,
    /// Commemoration versicle.
    CommemorationVersicle,
    /// Commemoration collect.
    CommemorationCollect,
    /// Conclusion.
    Conclusion,
    /// Final Marian antiphon.
    MarianAntiphon,
    /// Mass introit.
    Introit,
    /// Kyrie.
    Kyrie,
    /// Gloria.
    Gloria,
    /// Epistle.
    Epistle,
    /// Gradual.
    Gradual,
    /// Alleluia.
    Alleluia,
    /// Tract.
    Tract,
    /// Sequence.
    Sequence,
    /// Gospel.
    Gospel,
    /// Creed.
    Creed,
    /// Offertory.
    Offertory,
    /// Secret.
    Secret,
    /// Preface.
    Preface,
    /// Communion.
    Communion,
    /// Postcommunion.
    Postcommunion,
    /// Last Gospel.
    LastGospel,
    /// Martyrology heading.
    MartyrologyHeading,
    /// Martyrology entry.
    MartyrologyEntry,
    /// Rubric or rule metadata.
    Rubric,
    /// General note.
    Note,
}

/// A typed psalm or canticle reference.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
pub struct PsalmReference {
    /// Psalm or canticle number.
    pub number: String,
    /// Optional first verse label.
    #[serde(default)]
    pub start: Option<String>,
    /// Optional last verse label.
    #[serde(default)]
    pub end: Option<String>,
    /// Whether this psalm is conditional for profiles that include optional psalmody.
    #[serde(default)]
    pub optional: bool,
}

/// Normalized rule token.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[non_exhaustive]
pub enum RuleToken {
    /// Boolean rule flag.
    Flag {
        /// Stable flag identifier.
        id: String,
        /// Human-readable source label.
        label: String,
    },
    /// Source reference such as a common.
    SourceRef {
        /// Reference relation.
        relation: String,
        /// Target source key.
        target: String,
    },
    /// Key-value rule token.
    Value {
        /// Stable key.
        key: String,
        /// Value.
        value: String,
        /// Human-readable source label.
        label: String,
    },
}

impl RuleToken {
    fn label(&self) -> &str {
        match self {
            Self::Flag { label, .. } | Self::Value { label, .. } => label,
            Self::SourceRef { target, .. } => target,
        }
    }
}

/// Canonical content node stored in YAML.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ContentNode {
    /// Literal text.
    Text {
        /// Text payload.
        text: String,
    },
    /// Rubric or direction.
    Rubric {
        /// Rubric payload.
        text: String,
    },
    /// Citation or marker.
    Marker {
        /// Marker payload.
        text: String,
    },
    /// Section heading.
    Heading {
        /// Heading text.
        text: String,
    },
    /// Biblical or liturgical citation.
    Citation {
        /// Citation text.
        text: String,
    },
    /// Versicle.
    Versicle {
        /// Versicle text.
        text: String,
    },
    /// Response.
    Response {
        /// Response text.
        text: String,
    },
    /// Short responsory.
    ShortResponse {
        /// Short responsory text.
        text: String,
    },
    /// Prayer or collect body.
    Prayer {
        /// Prayer text.
        text: String,
    },
    /// Blessing.
    Blessing {
        /// Blessing text.
        text: String,
    },
    /// Antiphon text.
    Antiphon {
        /// Antiphon payload.
        text: String,
    },
    /// Psalm or canticle reference.
    PsalmRef {
        /// Psalm number.
        number: String,
        /// Optional first verse.
        #[serde(default)]
        start: Option<String>,
        /// Optional last verse.
        #[serde(default)]
        end: Option<String>,
        /// Whether this psalm is conditional for profiles that include optional psalmody.
        #[serde(default)]
        optional: bool,
    },
    /// Complete antiphon-plus-psalm entry.
    Psalmody {
        /// Antiphon.
        antiphon: String,
        /// Psalm references.
        psalms: Vec<PsalmReference>,
    },
    /// Named table row.
    TableRow {
        /// Row label.
        label: String,
        /// Row text, normally an antiphon.
        #[serde(default)]
        text: Option<String>,
        /// Psalm references in the row.
        #[serde(default)]
        psalms: Vec<PsalmReference>,
    },
    /// Rank metadata.
    Rank {
        /// Rank label.
        #[serde(default)]
        label: Option<String>,
        /// Rank value.
        #[serde(default)]
        value: Option<OrderedFloat<f32>>,
        /// Common reference.
        #[serde(default)]
        common: Option<String>,
    },
    /// Rule metadata.
    Rule {
        /// Rule tokens.
        tokens: Vec<RuleToken>,
    },
}

/// Render-neutral output node.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum DocumentNode {
    /// Literal text.
    Text {
        /// Text payload.
        text: String,
    },
    /// Section heading.
    Heading {
        /// Heading text.
        text: String,
    },
    /// Rubric or direction.
    Rubric {
        /// Rubric text.
        text: String,
    },
    /// Citation or block marker.
    Marker {
        /// Marker payload.
        text: String,
    },
    /// Biblical or liturgical citation.
    Citation {
        /// Citation text.
        text: String,
    },
    /// Versicle.
    Versicle {
        /// Versicle text without the leading `V.`.
        text: String,
    },
    /// Response.
    Response {
        /// Response text without the leading `R.`.
        text: String,
    },
    /// Short responsory.
    ShortResponse {
        /// Short responsory text without the leading `R.br.`.
        text: String,
    },
    /// Antiphon.
    Antiphon {
        /// Antiphon text without the leading `Ant.`.
        text: String,
    },
    /// Prayer or collect body.
    Prayer {
        /// Prayer text.
        text: String,
    },
    /// Blessing.
    Blessing {
        /// Blessing text without the leading `Benedictio.`.
        text: String,
    },
    /// Amen response.
    Amen,
    /// Unresolved reference, retained for diagnostics.
    Unresolved {
        /// Reference kind.
        kind: String,
        /// Reference value.
        value: String,
        /// Reason.
        reason: String,
    },
}

impl DocumentNode {
    /// Returns the stable semantic kind for this output node.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Text { .. } => "text",
            Self::Heading { .. } => "heading",
            Self::Rubric { .. } => "rubric",
            Self::Marker { .. } => "marker",
            Self::Citation { .. } => "citation",
            Self::Versicle { .. } => "versicle",
            Self::Response { .. } => "response",
            Self::ShortResponse { .. } => "short_response",
            Self::Antiphon { .. } => "antiphon",
            Self::Prayer { .. } => "prayer",
            Self::Blessing { .. } => "blessing",
            Self::Amen => "amen",
            Self::Unresolved { .. } => "unresolved",
        }
    }

    /// Returns a plain-text rendering suitable for comparison and simple UIs.
    pub fn plain_text(&self) -> String {
        self.plain_text_for_language("la")
    }

    /// Returns a plain-text rendering for a specific language column.
    pub fn plain_text_for_language(&self, language: &str) -> String {
        match self {
            Self::Text { text }
            | Self::Heading { text }
            | Self::Rubric { text }
            | Self::Marker { text }
            | Self::Citation { text }
            | Self::Prayer { text } => text.clone(),
            Self::Versicle { text } => format!("V. {text}"),
            Self::Response { text } => format!("R. {text}"),
            Self::ShortResponse { text } => format!("R.br. {text}"),
            Self::Antiphon { text } => format!("Ant. {text}"),
            Self::Blessing { text } if language == "en" => format!("Benediction. {text}"),
            Self::Blessing { text } => format!("Benedictio. {text}"),
            Self::Amen => "R. Amen.".to_string(),
            Self::Unresolved {
                kind,
                value,
                reason,
            } => format!("[unresolved {kind}: {value}; {reason}]"),
        }
    }
}

/// Resolved or missing content for one side-by-side column.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum OfficeColumnContent {
    /// Content resolved for this language.
    Resolved {
        /// Output nodes.
        nodes: Vec<DocumentNode>,
    },
    /// This requested language is missing.
    Missing {
        /// Human-readable reason.
        reason: String,
    },
}

/// One language column in a block.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfficeColumn {
    /// Language identifier.
    pub language: LanguageId,
    /// Localized block title.
    pub title: Option<String>,
    /// Column content.
    pub content: OfficeColumnContent,
}

/// One block in a resolved Office document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfficeBlock {
    /// Stable block identifier.
    pub id: RecordId,
    /// Semantic role.
    pub role: TextRole,
    /// Side-by-side columns.
    pub columns: Vec<OfficeColumn>,
}

/// Non-fatal diagnostic.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Diagnostic {
    /// Stable diagnostic code.
    pub code: &'static str,
    /// Human-readable message.
    pub message: String,
}

/// Resolution trace event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceEvent {
    /// Phase name.
    pub phase: &'static str,
    /// Event message.
    pub message: String,
}

/// Date facts used by the Office resolver.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DateFacts {
    /// Gregorian date.
    pub date: NaiveDate,
    /// Weekday.
    pub weekday: Weekday,
    /// Gregorian Easter.
    pub easter: NaiveDate,
    /// Temporal week key.
    pub temporal_week: String,
    /// Temporal source stem.
    pub temporal_stem: String,
    /// Sanctoral fixed-date key.
    pub sanctoral_key: String,
}

/// Request for one Office hour.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfficeRequest {
    /// Gregorian date.
    pub date: NaiveDate,
    /// Office hour.
    pub hour: Hour,
    /// Rubrical profile.
    pub profile: ProfileId,
    /// Requested languages, displayed side by side.
    pub languages: Vec<LanguageId>,
}

impl OfficeRequest {
    /// Builds a Roman 1960 request with Latin and English columns.
    pub fn new(date: NaiveDate, hour: Hour) -> Self {
        Self {
            date,
            hour,
            profile: "roman-1960".to_string(),
            languages: vec!["la".to_string(), "en".to_string()],
        }
    }
}

/// Selected or candidate observance.
#[derive(Clone, Debug, PartialEq)]
pub struct OfficeObservance {
    /// Stable ID.
    pub id: RecordId,
    /// Title.
    pub title: Option<String>,
    /// Kind.
    pub kind: ObservanceKind,
    /// Numeric rank.
    pub rank: Option<f32>,
    /// Rank label.
    pub rank_label: Option<String>,
    catalog_key: Option<String>,
}

/// Structured Office result.
#[derive(Clone, Debug, PartialEq)]
pub struct OfficeDocument {
    /// Date facts.
    pub date_facts: DateFacts,
    /// Hour.
    pub hour: Hour,
    /// Profile.
    pub profile: ProfileId,
    /// Principal observance.
    pub principal: OfficeObservance,
    /// Temporal candidate.
    pub temporal: Option<OfficeObservance>,
    /// Sanctoral candidate.
    pub sanctoral: Option<OfficeObservance>,
    /// Commemorations.
    pub commemorations: Vec<OfficeObservance>,
    /// Output blocks.
    pub blocks: Vec<OfficeBlock>,
    /// Diagnostics.
    pub diagnostics: Vec<Diagnostic>,
    /// Trace events.
    pub trace: Vec<TraceEvent>,
}

/// Summary counts for the embedded catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogStats {
    /// Profile count.
    pub profiles: usize,
    /// Rite count.
    pub rites: usize,
    /// Text count.
    pub texts: usize,
}

/// Public view of a catalog text record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CatalogText {
    /// Text ID.
    pub id: RecordId,
    /// Language.
    pub language: LanguageId,
    /// Role.
    pub role: TextRole,
    /// Canonical content nodes.
    pub content: Vec<ContentNode>,
}

/// Public view of a reusable multilingual corpus text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CorpusText {
    /// Corpus text ID.
    pub id: RecordId,
    /// Semantic role of the text.
    pub role: TextRole,
    /// Language-specific content keyed by language ID.
    pub content: BTreeMap<LanguageId, Vec<ContentNode>>,
}

/// Immutable embedded catalog.
#[derive(Clone, Debug)]
pub struct Catalog {
    profiles: BTreeMap<ProfileId, RawProfile>,
    rites: BTreeMap<RecordId, RawRite>,
    skeletons: BTreeMap<(ProfileId, Hour), RawOfficeSkeleton>,
    corpus: BTreeMap<RecordId, RawCorpusRecord>,
    texts: BTreeMap<(LanguageId, RecordId), RawTextRecord>,
    source_key_index: BTreeMap<(LanguageId, String), Vec<RecordId>>,
}

impl Catalog {
    /// Returns summary counts.
    pub fn stats(&self) -> CatalogStats {
        CatalogStats {
            profiles: self.profiles.len(),
            rites: self.rites.len(),
            texts: self.texts.len(),
        }
    }

    /// Returns one localized text by ID.
    pub fn text(&self, language: &str, id: &str) -> Option<CatalogText> {
        self.texts
            .get(&(language.to_string(), id.to_string()))
            .map(|record| catalog_text(language, id, record))
    }

    /// Iterates all localized texts.
    pub fn texts(&self) -> impl Iterator<Item = CatalogText> + '_ {
        self.texts
            .iter()
            .map(|((language, id), record)| catalog_text(language, id, record))
    }

    /// Iterates reusable multilingual corpus texts.
    pub fn corpus_texts(&self) -> impl Iterator<Item = CorpusText> + '_ {
        self.corpus.iter().map(|(id, record)| CorpusText {
            id: id.clone(),
            role: record.role.clone(),
            content: record.content.clone(),
        })
    }

    fn texts_by_source_key(&self, language: &str, source_key: &str) -> Vec<CatalogText> {
        self.source_key_index
            .get(&(language.to_string(), source_key.to_string()))
            .into_iter()
            .flatten()
            .filter_map(|id| self.text(language, id))
            .collect()
    }
}

/// Embedded breviary engine.
#[derive(Clone, Copy, Debug)]
pub struct Breviarium {
    catalog: &'static Catalog,
}

impl Breviarium {
    /// Loads the embedded catalog.
    pub fn embedded() -> Result<Self, DataError> {
        Ok(Self {
            catalog: catalog()?,
        })
    }

    /// Returns the embedded catalog.
    pub fn catalog(&self) -> &'static Catalog {
        self.catalog
    }

    /// Resolves one Office hour.
    pub fn resolve_office(&self, request: OfficeRequest) -> Result<OfficeDocument, DataError> {
        resolve_office(self.catalog, request)
    }
}

/// Catalog loading and lookup errors.
#[derive(Debug, Error, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub enum DataError {
    /// Embedded file missing.
    #[error("embedded data file not found: {path}")]
    MissingEmbeddedFile {
        /// Path.
        path: String,
    },
    /// Embedded file was not UTF-8.
    #[error("embedded data file is not UTF-8: {path}")]
    NonUtf8Data {
        /// Path.
        path: String,
    },
    /// YAML parse failed.
    #[error("failed to parse YAML at {path}: {message}")]
    Yaml {
        /// Path.
        path: String,
        /// Message.
        message: String,
    },
    /// Catalog validation failed.
    #[error("catalog validation failed: {message}")]
    InvalidCatalog {
        /// Message.
        message: String,
    },
    /// Unsupported request scope.
    #[error("unsupported scope: {message}")]
    UnsupportedScope {
        /// Message.
        message: String,
    },
    /// Required text missing.
    #[error("missing required text: {message}")]
    MissingText {
        /// Message.
        message: String,
    },
}

/// Returns the lazily parsed embedded catalog.
pub fn catalog() -> Result<&'static Catalog, DataError> {
    CATALOG
        .get_or_init(load_catalog)
        .as_ref()
        .map_err(Clone::clone)
}

/// Computes Office date facts for a Gregorian date.
pub fn office_date_facts(date: NaiveDate) -> Result<DateFacts, DataError> {
    let gregorian_start = NaiveDate::from_ymd_opt(1582, 10, 15).expect("valid date");
    if date < gregorian_start {
        return Err(DataError::UnsupportedScope {
            message: "Office dates before October 15, 1582 are outside the Gregorian calendar"
                .to_string(),
        });
    }
    let temporal_week = temporal_week_key(date, false)?;
    let weekday = date.weekday();
    let day = divinum_weekday_number(weekday);
    let temporal_stem = if temporal_week.starts_with("Nat") {
        temporal_week.clone()
    } else {
        format!("{temporal_week}-{day}")
    };
    Ok(DateFacts {
        date,
        weekday,
        easter: gregorian_easter(date.year()).ok_or_else(|| DataError::UnsupportedScope {
            message: format!("could not compute Easter for {}", date.year()),
        })?,
        temporal_week,
        temporal_stem,
        sanctoral_key: sanctoral_key(date),
    })
}

fn catalog_text(language: &str, id: &str, record: &RawTextRecord) -> CatalogText {
    CatalogText {
        id: id.to_string(),
        language: language.to_string(),
        role: record.role.clone(),
        content: record.content.clone(),
    }
}

fn load_catalog() -> Result<Catalog, DataError> {
    let mut builder = CatalogBuilder::default();
    let mut paths = DATA_DIR
        .find("**/*.yaml")
        .map_err(|error| DataError::InvalidCatalog {
            message: format!("invalid embedded data glob: {error}"),
        })?
        .filter_map(|entry| entry.as_file())
        .map(|file| file.path().to_string_lossy().replace('\\', "/"))
        .collect::<Vec<_>>();
    paths.sort();
    for path in paths {
        let doc: RawDocument = load_yaml(&path)?;
        builder.insert(path, doc)?;
    }
    builder.finish()
}

fn load_yaml<T>(path: &str) -> Result<T, DataError>
where
    for<'de> T: Deserialize<'de>,
{
    let file = DATA_DIR
        .get_file(path)
        .ok_or_else(|| DataError::MissingEmbeddedFile {
            path: path.to_string(),
        })?;
    let text = file.contents_utf8().ok_or_else(|| DataError::NonUtf8Data {
        path: path.to_string(),
    })?;
    yaml_serde::from_str(text).map_err(|error| DataError::Yaml {
        path: path.to_string(),
        message: error.to_string(),
    })
}

fn resolve_office(catalog: &Catalog, request: OfficeRequest) -> Result<OfficeDocument, DataError> {
    let mut diagnostics = Vec::new();
    let mut trace = Vec::new();
    let profile =
        catalog
            .profiles
            .get(&request.profile)
            .ok_or_else(|| DataError::UnsupportedScope {
                message: format!("profile `{}` is not embedded", request.profile),
            })?;
    if !profile
        .supported_services
        .iter()
        .any(|service| service == "office")
    {
        return Err(DataError::UnsupportedScope {
            message: format!("profile `{}` does not support Office", profile.id),
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
        let rank_common = principal_key
            .as_deref()
            .and_then(|key| section_nodes(catalog, primary_language, key, "rank"))
            .and_then(|nodes| rank_from_nodes(&nodes).and_then(|rank| rank.common));
        let (rule_flags, rule_values, rule_source) = principal_key
            .as_deref()
            .and_then(|key| section_nodes(catalog, primary_language, key, "rules"))
            .map(|nodes| rule_maps(&nodes))
            .unwrap_or_default();
        let commune_ref = rule_source.or(rank_common);
        let commune_key = commune_ref
            .as_deref()
            .zip(principal_key.as_deref())
            .map(|(reference, source_key)| source_reference_key(source_key, reference));
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
                let rank_common = section_nodes(catalog, primary_language, &source_key, "rank")
                    .and_then(|nodes| rank_from_nodes(&nodes).and_then(|rank| rank.common));
                let (_, _, rule_source) =
                    section_nodes(catalog, primary_language, &source_key, "rules")
                        .map(|nodes| rule_maps(&nodes))
                        .unwrap_or_default();
                let commune_key = rule_source
                    .or(rank_common)
                    .map(|reference| source_reference_key(&source_key, &reference));
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

    fn principal_sources(&self) -> Vec<&str> {
        let mut sources = Vec::new();
        if let Some(key) = &self.principal_key {
            sources.push(key.as_str());
        }
        if let Some(key) = &self.commune_key {
            sources.push(key.as_str());
        }
        sources
    }

    fn collect_sources(&self) -> Vec<&str> {
        let mut sources = self.inherited_sources();
        for key in &self.collect_reference_keys {
            let source = key.as_str();
            if !sources.iter().any(|existing| *existing == source) {
                sources.push(source);
            }
        }
        sources
    }

    fn inherited_sources(&self) -> Vec<&str> {
        let mut sources = Vec::new();
        self.push_source(&mut sources, &self.principal_key);
        self.push_source(&mut sources, &self.commune_key);
        self.push_source(&mut sources, &self.temporal_key);
        self.push_source(&mut sources, &self.weekly_temporal_key);
        self.push_source(&mut sources, &self.previous_temporal_key);
        sources
    }

    fn matins_lesson_sources(&self) -> Vec<&str> {
        let mut sources = Vec::new();
        for key in [
            &self.principal_key,
            &self.scripture_key,
            &self.temporal_key,
            &self.weekly_temporal_key,
            &self.commune_key,
        ] {
            if let Some(key) = key {
                let source = key.as_str();
                if !sources.iter().any(|existing| *existing == source) {
                    sources.push(source);
                }
            }
        }
        sources
    }

    fn push_source<'a>(&'a self, sources: &mut Vec<&'a str>, file: &'a Option<String>) {
        if let Some(file) = file {
            let source = file.as_str();
            if !sources.iter().any(|existing| *existing == source) {
                sources.push(source);
            }
        }
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

    fn major_special_source(&self) -> String {
        "ordinary/major".to_string()
    }

    fn minor_special_source(&self) -> String {
        "ordinary/minor".to_string()
    }

    fn prime_special_source(&self) -> String {
        "ordinary/prime".to_string()
    }

    fn matins_special_source(&self) -> String {
        "ordinary/matins".to_string()
    }

    fn psalmi_major_source(&self) -> String {
        "psalter/major".to_string()
    }

    fn psalmi_minor_source(&self) -> String {
        "psalter/minor".to_string()
    }

    fn psalmi_matutinum_source(&self) -> String {
        "psalter/matins".to_string()
    }

    fn benedictions_source(&self) -> String {
        "ordinary/benedictions".to_string()
    }

    fn maria_antiphon_source(&self) -> String {
        "ordinary/marian-antiphons".to_string()
    }
}

fn execute_steps(
    catalog: &Catalog,
    request: &OfficeRequest,
    context: &OfficeContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<OfficeBlock> {
    let steps = catalog
        .skeletons
        .get(&(request.profile.clone(), request.hour))
        .map(|skeleton| skeleton.steps.clone())
        .unwrap_or_else(|| builtin_steps(request.hour));
    steps
        .iter()
        .map(|step| {
            let columns = request
                .languages
                .iter()
                .map(|language| {
                    let content =
                        match resolve_step(catalog, language, context, &step.kind, diagnostics) {
                            Ok(nodes) => OfficeColumnContent::Resolved { nodes },
                            Err(reason) => OfficeColumnContent::Missing { reason },
                        };
                    OfficeColumn {
                        language: language.clone(),
                        title: step.titles.get(language).cloned(),
                        content,
                    }
                })
                .collect();
            OfficeBlock {
                id: format!(
                    "office.{}.{}.{}",
                    request.profile,
                    request.hour.as_str(),
                    step.id
                ),
                role: step.role.clone(),
                columns,
            }
        })
        .collect()
}

fn resolve_step(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    kind: &OfficeStepKind,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    match kind {
        OfficeStepKind::Opening => resolve_opening(catalog, language, context, diagnostics),
        OfficeStepKind::MatinsOpening => {
            resolve_matins_opening(catalog, language, context, diagnostics)
        }
        OfficeStepKind::MatinsInvitatory => {
            resolve_matins_invitatory(catalog, language, context, diagnostics)
        }
        OfficeStepKind::MatinsHymn => resolve_matins_hymn(catalog, language, context, diagnostics),
        OfficeStepKind::MatinsNocturns => {
            resolve_matins_nocturns(catalog, language, context, diagnostics)
        }
        OfficeStepKind::LaudsPsalmody => {
            resolve_major_psalmody(catalog, language, context, Hour::Lauds, diagnostics)
        }
        OfficeStepKind::MajorChapterHymnVerse => {
            resolve_major_chapter_hymn_verse(catalog, language, context, Hour::Lauds, diagnostics)
        }
        OfficeStepKind::GospelCanticle => {
            resolve_gospel_canticle(catalog, language, context, diagnostics)
        }
        OfficeStepKind::VespersPsalmody => {
            resolve_major_psalmody(catalog, language, context, Hour::Vespers, diagnostics)
        }
        OfficeStepKind::VespersChapterHymnVerse => {
            resolve_major_chapter_hymn_verse(catalog, language, context, Hour::Vespers, diagnostics)
        }
        OfficeStepKind::Magnificat => resolve_magnificat(catalog, language, context, diagnostics),
        OfficeStepKind::PrimeHymn => section_doc(
            catalog,
            language,
            context,
            &context.prime_special_source(),
            "prime-hymn",
            diagnostics,
        ),
        OfficeStepKind::MinorHymn => resolve_minor_hymn(catalog, language, context, diagnostics),
        OfficeStepKind::MinorPsalmody => {
            resolve_minor_psalmody(catalog, language, context, diagnostics)
        }
        OfficeStepKind::MinorChapterResponsoryVerse => {
            resolve_minor_chapter_responsory_verse(catalog, language, context, diagnostics)
        }
        OfficeStepKind::PrimeCollect => resolve_prime_collect(catalog, language, diagnostics),
        OfficeStepKind::PrimeMartyrology => {
            resolve_prime_martyrology(catalog, language, context, diagnostics)
        }
        OfficeStepKind::PrimePretiosa => formula_nodes(catalog, language, "Pretiosa", diagnostics),
        OfficeStepKind::PrimeChapterOffice => {
            resolve_prime_chapter_office(catalog, language, diagnostics)
        }
        OfficeStepKind::PrimeShortReading => {
            resolve_prime_short_reading(catalog, language, context, diagnostics)
        }
        OfficeStepKind::PrimeConclusion => resolve_prime_conclusion(catalog, language, diagnostics),
        OfficeStepKind::ComplineOpening => resolve_compline_opening(catalog, language, diagnostics),
        OfficeStepKind::ComplineShortReading => section_doc(
            catalog,
            language,
            context,
            &context.minor_special_source(),
            "compline-short-reading",
            diagnostics,
        ),
        OfficeStepKind::ComplineExamination => {
            resolve_compline_examination(catalog, language, diagnostics)
        }
        OfficeStepKind::ComplinePsalmody => {
            resolve_compline_psalmody(catalog, language, context, diagnostics)
        }
        OfficeStepKind::ComplineHymn => {
            resolve_compline_hymn(catalog, language, context, diagnostics)
        }
        OfficeStepKind::ComplineChapterResponsoryVerse => {
            resolve_compline_chapter_responsory_verse(catalog, language, context, diagnostics)
        }
        OfficeStepKind::NuncDimittis => {
            resolve_nunc_dimittis(catalog, language, context, diagnostics)
        }
        OfficeStepKind::ComplineCollect => resolve_compline_collect(catalog, language, diagnostics),
        OfficeStepKind::ComplineConclusion => {
            resolve_compline_conclusion(catalog, language, diagnostics)
        }
        OfficeStepKind::Preces => Ok(vec![DocumentNode::Marker {
            text: localized_literal(language, "omittitur", "omit").to_string(),
        }]),
        OfficeStepKind::Collects => resolve_collects(catalog, language, context, diagnostics),
        OfficeStepKind::Conclusion => resolve_conclusion(catalog, language, diagnostics),
        OfficeStepKind::FinalAntiphon => {
            resolve_final_antiphon(catalog, language, context, diagnostics)
        }
        OfficeStepKind::Unsupported => Err("unsupported step".to_string()),
    }
}

fn resolve_opening(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let mut nodes = formula_nodes(catalog, language, "Deus in adjutorium", diagnostics)?;
    let alleluia = formula_lines(catalog, language, "Alleluia", diagnostics)?;
    let index = usize::from(context.facts.temporal_week.starts_with("Quad"));
    if let Some(line) = alleluia.get(index) {
        nodes.push(DocumentNode::Text { text: line.clone() });
    }
    Ok(nodes)
}

fn resolve_matins_opening(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let mut nodes = formula_nodes(catalog, language, "Domine labia", diagnostics)?;
    nodes.extend(resolve_opening(catalog, language, context, diagnostics)?);
    Ok(nodes)
}

fn resolve_matins_invitatory(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let antiphon = first_section_antiphons(
        catalog,
        language,
        &context.principal_sources(),
        "matins-invitatory",
    )
    .or_else(|| section_antiphons(catalog, language, &context.matins_special_source(), "Invit"))
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

fn resolve_matins_hymn(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    first_section_doc(
        catalog,
        language,
        context,
        &context.principal_sources(),
        "matins-hymn",
        diagnostics,
    )
    .or_else(|_| {
        let section = matins_ordinary_hymn_section(context);
        section_doc(
            catalog,
            language,
            context,
            &context.matins_special_source(),
            &section,
            diagnostics,
        )
    })
}

fn resolve_matins_nocturns(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let entries = first_section_psalmody(
        catalog,
        language,
        &context.principal_sources(),
        "matins-psalmody",
    )
    .or_else(|| {
        section_psalmody(
            catalog,
            language,
            &context.psalmi_matutinum_source(),
            &format!("Day{}", divinum_weekday_number(context.facts.weekday)),
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
        .principal_sources()
        .iter()
        .any(|source| section_nodes(catalog, language, source, "matins-reading-4").is_some())
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
    let sources = context.principal_sources();
    let has_lectio94 = sources.iter().any(|source| {
        section_nodes(catalog, language, source, "matins-reading-3-abbreviated").is_some()
    });
    let has_lectio4 = sources
        .iter()
        .any(|source| section_nodes(catalog, language, source, "matins-reading-4").is_some());
    has_lectio94 && !has_lectio4
}

fn resolve_matins_versicle(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    nocturn: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let section = format!("matins-nocturn-{nocturn}-versicle");
    first_section_doc(
        catalog,
        language,
        context,
        &context.principal_sources(),
        &section,
        diagnostics,
    )
    .or_else(|_| {
        let pairs = section_nodes(
            catalog,
            language,
            &context.psalmi_matutinum_source(),
            &format!("Day{}", divinum_weekday_number(context.facts.weekday)),
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
        context,
        nocturn,
        diagnostics,
    )?);
    let lesson_sources = context.matins_lesson_sources();
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
                    section_doc(catalog, language, context, key, &section, diagnostics)
                {
                    nodes.extend(lectio);
                    lesson_added = true;
                }
            }
        }
        if !lesson_added && use_abbreviated_sanctoral_lesson && lesson == 3 {
            if let Ok(lectio) = first_section_doc(
                catalog,
                language,
                context,
                &context.principal_sources(),
                "matins-reading-3-abbreviated",
                diagnostics,
            ) {
                nodes.extend(lectio);
                lesson_added = true;
            }
        }
        if !lesson_added {
            match first_section_doc(
                catalog,
                language,
                context,
                &lesson_sources,
                &section,
                diagnostics,
            ) {
                Ok(lectio) => nodes.extend(lectio),
                Err(reason) => nodes.push(DocumentNode::Unresolved {
                    kind: "section".to_string(),
                    value: section,
                    reason,
                }),
            }
        }
        if let Ok(resp) = first_section_doc(
            catalog,
            language,
            context,
            &lesson_sources,
            &format!("matins-responsory-{lesson}"),
            diagnostics,
        ) {
            nodes.extend(resp);
        } else if lesson == 9 || (use_abbreviated_sanctoral_lesson && lesson == 3) {
            nodes.extend(formula_nodes(catalog, language, "Te Deum", diagnostics)?);
        }
    }
    Ok(nodes)
}

fn resolve_matins_absolution(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    nocturn: usize,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let mut nodes = formula_nodes(catalog, language, "Pater noster Et", diagnostics)?;
    if let Some(line) = section_lines(
        catalog,
        language,
        &context.benedictions_source(),
        "matins-absolutions",
    )
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
    let mut nodes = formula_nodes(catalog, language, "Jube domne", diagnostics)?;
    let section = match nocturn {
        1 => "matins-blessings-nocturn-1",
        2 => "matins-blessings-nocturn-2",
        _ if context.has_rule("lectio1-tempnat") => "matins-blessings-nocturn-3-christmas",
        _ => "matins-blessings-nocturn-3",
    };
    if let Some(line) = section_lines(catalog, language, &context.benedictions_source(), section)
        .and_then(|lines| lines.get((lesson - 1) % 3).cloned())
    {
        nodes.push(DocumentNode::Text {
            text: format!("Benedictio. {line}"),
        });
    }
    Ok(nodes)
}

fn resolve_major_psalmody(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    hour: Hour,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let (proper_section, ordinary_section) = match hour {
        Hour::Lauds => (
            "lauds-psalmody",
            format!(
                "Day{} Laudes{}",
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
                "Day{} Vespera",
                if context.has_rule("psalmi-dominica") {
                    0
                } else {
                    divinum_weekday_number(context.facts.weekday)
                }
            ),
        ),
        _ => return Err("not a major psalmody hour".to_string()),
    };
    let ordinary = section_psalmody(
        catalog,
        language,
        &context.psalmi_major_source(),
        &ordinary_section,
    )
    .ok_or_else(|| format!("missing ordinary psalmody `{ordinary_section}`"))?;
    let proper_entries = first_section_psalmody(
        catalog,
        language,
        &context.principal_sources(),
        proper_section,
    );
    let proper_antiphons = first_section_antiphons(
        catalog,
        language,
        &context.principal_sources(),
        proper_section,
    );
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

fn resolve_major_chapter_hymn_verse(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    hour: Hour,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let is_vespers = hour == Hour::Vespers;
    let chapter_candidates: &[&str] = if is_vespers {
        &["vespers-chapter", "lauds-chapter"]
    } else {
        &["lauds-chapter"]
    };
    let hymn_candidates: &[&str] = if is_vespers {
        &["vespers-hymn"]
    } else {
        &["lauds-hymn"]
    };
    let verse_candidates: &[&str] = if is_vespers {
        &["vespers-versicle", "lauds-versicle"]
    } else {
        &["lauds-versicle"]
    };
    let inherited_sources = context.inherited_sources();
    let special_source = context.major_special_source();
    let special_sources = [special_source.as_str()];
    let mut nodes = match first_of_sections(
        catalog,
        language,
        context,
        &inherited_sources,
        chapter_candidates,
        diagnostics,
    )
    .or_else(|_| {
        let sections = if is_vespers {
            if context.facts.weekday == Weekday::Sun {
                vec!["Dominica Vespera", "Responsory Dominica Vespera"]
            } else {
                vec!["Feria Vespera", "Responsory Feria Vespera"]
            }
        } else if context.facts.weekday == Weekday::Sun {
            vec!["Dominica Laudes"]
        } else {
            vec!["Feria Laudes"]
        };
        first_of_sections(
            catalog,
            language,
            context,
            &special_sources,
            &sections,
            diagnostics,
        )
    }) {
        Ok(chapter) => chapter,
        Err(reason) => vec![DocumentNode::Unresolved {
            kind: "section".to_string(),
            value: if is_vespers {
                "major vespers chapter"
            } else {
                "major lauds chapter"
            }
            .to_string(),
            reason,
        }],
    };
    match first_of_sections(
        catalog,
        language,
        context,
        &inherited_sources,
        hymn_candidates,
        diagnostics,
    )
    .or_else(|_| {
        let sections = major_hymn_fallback_sections(context, is_vespers);
        let section_refs = sections.iter().map(String::as_str).collect::<Vec<_>>();
        first_of_sections(
            catalog,
            language,
            context,
            &special_sources,
            &section_refs,
            diagnostics,
        )
    })
    .or_else(|_| major_external_hymn_fallback(catalog, language, context, is_vespers, diagnostics))
    {
        Ok(hymn) => nodes.extend(hymn),
        Err(reason) => nodes.push(DocumentNode::Unresolved {
            kind: "section".to_string(),
            value: if is_vespers {
                "major vespers hymn"
            } else {
                "major lauds hymn"
            }
            .to_string(),
            reason,
        }),
    }
    match first_of_sections(
        catalog,
        language,
        context,
        &inherited_sources,
        verse_candidates,
        diagnostics,
    )
    .or_else(|_| {
        let section = if is_vespers {
            if context.facts.weekday == Weekday::Sun {
                "Dominica Versum 3"
            } else {
                "Feria Versum 3"
            }
        } else if context.facts.weekday == Weekday::Sun {
            "Dominica Versum 2"
        } else {
            "Feria Versum 2"
        };
        section_doc(
            catalog,
            language,
            context,
            &special_source,
            section,
            diagnostics,
        )
    }) {
        Ok(verse) => nodes.extend(verse),
        Err(reason) => nodes.push(DocumentNode::Unresolved {
            kind: "section".to_string(),
            value: if is_vespers {
                "major vespers versicle"
            } else {
                "major lauds versicle"
            }
            .to_string(),
            reason,
        }),
    }
    Ok(nodes)
}

fn major_hymn_fallback_sections(context: &OfficeContext, is_vespers: bool) -> Vec<String> {
    let hour_name = if is_vespers { "Vespera" } else { "Laudes" };
    let season = if context.facts.temporal_week.starts_with("Adv") {
        Some("Adv")
    } else if context.facts.temporal_week.starts_with("Quad5") {
        Some("Quad5")
    } else if context.facts.temporal_week.starts_with("Quad") {
        Some("Quad")
    } else if context.facts.temporal_week.starts_with("Pasc") {
        Some("Pasch")
    } else {
        None
    };
    if let Some(season) = season {
        return vec![
            format!("HymnusM {season} {hour_name}"),
            format!("Hymnus {season} {hour_name}"),
        ];
    }
    let weekday = divinum_weekday_number(context.facts.weekday);
    let mut sections = vec![format!("Hymnus Day{weekday} {hour_name}")];
    if is_vespers && weekday == 6 {
        sections.push("HymnusM Day6 Vespera".to_string());
    }
    sections
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
    let trinity_sunday_source = source_key(&["proper", "temporal", "Pent01-0"]);
    first_of_sections(
        catalog,
        language,
        context,
        &[trinity_sunday_source.as_str()],
        &["vespers-hymn"],
        diagnostics,
    )
}

fn resolve_gospel_canticle(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let antiphon = first_section_antiphons(
        catalog,
        language,
        &context.principal_sources(),
        "lauds-gospel-antiphon",
    )
    .or_else(|| {
        section_antiphons(
            catalog,
            language,
            &context.major_special_source(),
            &ferial_benedictus_antiphon_section(context),
        )
    })
    .and_then(|values| values.into_iter().next())
    .unwrap_or_default();
    gospel_canticle_nodes(catalog, language, "231", &antiphon, diagnostics)
}

fn resolve_magnificat(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let antiphon = first_section_antiphons(
        catalog,
        language,
        &context.principal_sources(),
        "vespers-gospel-antiphon",
    )
    .or_else(|| {
        section_antiphons(
            catalog,
            language,
            &context.major_special_source(),
            &ferial_magnificat_antiphon_section(context),
        )
    })
    .and_then(|values| values.into_iter().next())
    .unwrap_or_default();
    gospel_canticle_nodes(catalog, language, "232", &antiphon, diagnostics)
}

fn resolve_minor_hymn(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let hour = minor_hour_name(context.hour).ok_or_else(|| "not a minor hour".to_string())?;
    section_doc(
        catalog,
        language,
        context,
        &context.minor_special_source(),
        &format!("Hymnus {hour}"),
        diagnostics,
    )
}

fn resolve_minor_psalmody(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let hour = minor_hour_name(context.hour).ok_or_else(|| "not a minor hour".to_string())?;
    let label = minor_hour_row_label(context.hour, context)
        .ok_or_else(|| "not a minor hour".to_string())?;
    let row = table_row_with_fallbacks(
        catalog,
        language,
        &context.psalmi_minor_source(),
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
    let mut entries = vec![entry];
    if context.omits_optional_psalms() {
        for entry in &mut entries {
            entry.psalms.retain(|psalm| !psalm.optional);
        }
    }
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

fn resolve_minor_chapter_responsory_verse(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    if context.hour == Hour::Prime {
        let mut nodes = section_doc(
            catalog,
            language,
            context,
            &context.prime_special_source(),
            if context.facts.weekday == Weekday::Sun || context.has_rule("psalmi-dominica") {
                "Dominica"
            } else {
                "Feria"
            },
            diagnostics,
        )?;
        nodes.extend(section_doc(
            catalog,
            language,
            context,
            &context.prime_special_source(),
            "prime-short-responsory",
            diagnostics,
        )?);
        if let Ok(seasonal) = section_doc(
            catalog,
            language,
            context,
            &context.prime_special_source(),
            &format!("Responsory {}", prime_season(context)),
            diagnostics,
        ) {
            nodes.extend(seasonal);
        }
        nodes.extend(section_doc(
            catalog,
            language,
            context,
            &context.prime_special_source(),
            "prime-versicle",
            diagnostics,
        )?);
        return Ok(nodes);
    }

    let hour = minor_hour_name(context.hour).ok_or_else(|| "not a minor hour".to_string())?;
    let canonical_hour =
        canonical_minor_hour(context.hour).ok_or_else(|| "not a minor hour".to_string())?;
    let chapter_candidates = match context.hour {
        Hour::Terce => vec![
            format!("{canonical_hour}-chapter"),
            "lauds-chapter".to_string(),
        ],
        _ => vec![format!("{canonical_hour}-chapter")],
    };
    let mut nodes = chapter_candidates
        .iter()
        .find_map(|section| {
            first_section_doc(
                catalog,
                language,
                context,
                &context.principal_sources(),
                section,
                diagnostics,
            )
            .ok()
        })
        .or_else(|| {
            section_doc(
                catalog,
                language,
                context,
                &context.minor_special_source(),
                &format!("{} {hour}", minor_special_season(context)),
                diagnostics,
            )
            .ok()
        })
        .ok_or_else(|| format!("missing minor chapter for {hour}"))?;
    for (section, fallback_prefix, optional) in [
        (
            format!("{canonical_hour}-short-responsory"),
            "Responsory breve",
            false,
        ),
        (format!("{canonical_hour}-versicle"), "Versum", true),
    ] {
        if let Some(nodes2) = [section.as_str()].iter().find_map(|section| {
            first_section_doc(
                catalog,
                language,
                context,
                &context.principal_sources(),
                section,
                diagnostics,
            )
            .ok()
        }) {
            nodes.extend(nodes2);
        } else {
            let section = format!("{fallback_prefix} {} {hour}", minor_special_season(context));
            match section_doc(
                catalog,
                language,
                context,
                &context.minor_special_source(),
                &section,
                diagnostics,
            ) {
                Ok(nodes2) => nodes.extend(nodes2),
                Err(_) if optional => {}
                Err(error) => return Err(error),
            }
        }
    }
    Ok(nodes)
}

fn resolve_prime_collect(
    catalog: &Catalog,
    language: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let mut nodes = domine_exaudi_nodes(language);
    nodes.extend(formula_nodes(catalog, language, "Oremus", diagnostics)?);
    nodes.extend(formula_nodes(
        catalog,
        language,
        "oratio_Domine",
        diagnostics,
    )?);
    nodes.extend(formula_nodes(
        catalog,
        language,
        "Per Dominum",
        diagnostics,
    )?);
    nodes.extend(domine_exaudi_nodes(language));
    nodes.extend(formula_nodes(
        catalog,
        language,
        "Benedicamus Domino",
        diagnostics,
    )?);
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
    match section_doc(catalog, language, context, &source, "raw", diagnostics) {
        Ok(nodes) => Ok(nodes),
        Err(reason) => Ok(vec![DocumentNode::Unresolved {
            kind: "section".to_string(),
            value: format!("martyrology {key}"),
            reason,
        }]),
    }
}

fn resolve_prime_chapter_office(
    catalog: &Catalog,
    language: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let mut nodes = formula_nodes(catalog, language, "Deus in adjutorium iij", diagnostics)?;
    nodes.extend(formula_nodes(catalog, language, "Gloria", diagnostics)?);
    nodes.extend(formula_nodes(catalog, language, "Kyrie", diagnostics)?);
    nodes.extend(formula_nodes(
        catalog,
        language,
        "Pater noster Et",
        diagnostics,
    )?);
    nodes.extend(formula_nodes(catalog, language, "respice", diagnostics)?);
    nodes.extend(formula_nodes(catalog, language, "Oremus", diagnostics)?);
    nodes.extend(formula_nodes(catalog, language, "dirigere", diagnostics)?);
    Ok(nodes)
}

fn resolve_prime_short_reading(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let mut nodes = formula_nodes(catalog, language, "Jube domne", diagnostics)?;
    nodes.extend(formula_nodes(
        catalog,
        language,
        "benedictio Prima",
        diagnostics,
    )?);
    nodes.extend(
        first_section_doc(
            catalog,
            language,
            context,
            &context.principal_sources(),
            "prime-short-reading",
            diagnostics,
        )
        .or_else(|_| {
            section_doc(
                catalog,
                language,
                context,
                &context.prime_special_source(),
                prime_season(context),
                diagnostics,
            )
        })?,
    );
    Ok(nodes)
}

fn resolve_prime_conclusion(
    catalog: &Catalog,
    language: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let mut nodes = formula_nodes(catalog, language, "Adjutorium nostrum", diagnostics)?;
    nodes.extend(formula_nodes(catalog, language, "Benedicite", diagnostics)?);
    nodes.extend(formula_nodes(
        catalog,
        language,
        "benedictio Prima2",
        diagnostics,
    )?);
    Ok(nodes)
}

fn resolve_compline_opening(
    catalog: &Catalog,
    language: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let mut nodes = formula_nodes(catalog, language, "Jube domne", diagnostics)?;
    nodes.extend(formula_nodes(
        catalog,
        language,
        "Benedictio Completorium",
        diagnostics,
    )?);
    nodes.extend(amen_nodes());
    Ok(nodes)
}

fn resolve_compline_examination(
    catalog: &Catalog,
    language: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let mut nodes = formula_nodes(catalog, language, "Adjutorium nostrum", diagnostics)?;
    nodes.push(DocumentNode::Rubric {
        text: localized_literal(
            language,
            "Examen conscientiae vel Pater Noster totum secreto.",
            "There follows an examination of conscience, or the Our Father said silently.",
        )
        .to_string(),
    });
    nodes.extend(formula_nodes(
        catalog,
        language,
        "Pater noster",
        diagnostics,
    )?);
    nodes.extend(formula_nodes(catalog, language, "Confiteor", diagnostics)?);
    nodes.extend(formula_nodes(catalog, language, "Misereatur", diagnostics)?);
    nodes.extend(formula_nodes(
        catalog,
        language,
        "Indulgentiam",
        diagnostics,
    )?);
    nodes.extend(formula_nodes(
        catalog,
        language,
        "Converte nos",
        diagnostics,
    )?);
    Ok(nodes)
}

fn resolve_compline_psalmody(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let label = weekday_table_label(context.facts.weekday);
    let row = table_row(
        catalog,
        language,
        &context.psalmi_minor_source(),
        "Completorium",
        label,
    )
    .ok_or_else(|| format!("missing Compline psalmody row `{label}`"))?;
    expand_psalmody_entry(
        catalog,
        language,
        &PsalmodyEntry {
            antiphon: row.text.unwrap_or_default(),
            psalms: row.psalms,
        },
        diagnostics,
    )
}

fn resolve_compline_hymn(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let season = if context.facts.temporal_week.starts_with("Quad5") {
        "Hymnus Completorium Quad5"
    } else if context.facts.temporal_week.starts_with("Quad") {
        "Hymnus Completorium Quad"
    } else if context.facts.temporal_week.starts_with("Pasc") {
        "Hymnus Completorium Pasch"
    } else {
        "Hymnus Completorium"
    };
    section_doc(
        catalog,
        language,
        context,
        &context.minor_special_source(),
        season,
        diagnostics,
    )
    .or_else(|_| {
        section_doc(
            catalog,
            language,
            context,
            &context.minor_special_source(),
            "Hymnus Completorium",
            diagnostics,
        )
    })
}

fn resolve_compline_chapter_responsory_verse(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let mut nodes = section_doc(
        catalog,
        language,
        context,
        &context.minor_special_source(),
        "compline-chapter",
        diagnostics,
    )?;
    nodes.extend(section_doc(
        catalog,
        language,
        context,
        &context.minor_special_source(),
        "compline-short-responsory",
        diagnostics,
    )?);
    nodes.extend(section_doc(
        catalog,
        language,
        context,
        &context.minor_special_source(),
        "compline-versicle",
        diagnostics,
    )?);
    Ok(nodes)
}

fn resolve_nunc_dimittis(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let antiphon = section_antiphons(
        catalog,
        language,
        &context.minor_special_source(),
        compline_antiphon_section(context),
    )
    .or_else(|| {
        section_antiphons(
            catalog,
            language,
            &context.minor_special_source(),
            "compline-gospel-antiphon",
        )
    })
    .and_then(|values| values.into_iter().next())
    .unwrap_or_default();
    gospel_canticle_nodes(catalog, language, "233", &antiphon, diagnostics)
}

fn resolve_compline_collect(
    catalog: &Catalog,
    language: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let mut nodes = domine_exaudi_nodes(language);
    nodes.extend(formula_nodes(catalog, language, "Oremus", diagnostics)?);
    nodes.extend(formula_nodes(
        catalog,
        language,
        "oratio_Visita",
        diagnostics,
    )?);
    nodes.extend(formula_nodes(
        catalog,
        language,
        "Per Dominum",
        diagnostics,
    )?);
    Ok(nodes)
}

fn resolve_compline_conclusion(
    catalog: &Catalog,
    language: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let mut nodes = domine_exaudi_nodes(language);
    nodes.extend(formula_nodes(
        catalog,
        language,
        "Benedicamus Domino",
        diagnostics,
    )?);
    nodes.extend(first_formula_doc(
        catalog,
        language,
        &["benedictio Completorium Final", "Benedictio Completorium2"],
        diagnostics,
    )?);
    nodes.extend(amen_nodes());
    Ok(nodes)
}

fn resolve_collects(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let mut nodes = domine_exaudi_nodes(language);
    nodes.extend(formula_nodes(catalog, language, "Oremus", diagnostics)?);
    match first_collect_doc(
        catalog,
        language,
        context,
        &context.collect_sources(),
        diagnostics,
    ) {
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
    if let Some(antiphon) = first_section_antiphons(catalog, language, &sources, indexed_antiphon)
        .and_then(|mut values| values.pop())
    {
        nodes.push(DocumentNode::Text {
            text: format!("Ant. {}", close_antiphon(&antiphon)),
        });
    }
    if let Ok(versicle) = first_section_doc(
        catalog,
        language,
        context,
        &sources,
        indexed_versicle,
        diagnostics,
    ) {
        nodes.extend(versicle);
    }
    nodes.extend(formula_nodes(catalog, language, "Oremus", diagnostics)?);
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
    sources: &[&str],
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let sections = collect_section_candidates(context.hour);
    for source in sources {
        for section in &sections {
            if let Ok(nodes) = section_doc(catalog, language, context, source, section, diagnostics)
            {
                return Ok(nodes);
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

fn resolve_conclusion(
    catalog: &Catalog,
    language: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let mut nodes = domine_exaudi_nodes(language);
    nodes.extend(formula_nodes(
        catalog,
        language,
        "Benedicamus Domino",
        diagnostics,
    )?);
    nodes.extend(formula_nodes(
        catalog,
        language,
        "Fidelium animae",
        diagnostics,
    )?);
    Ok(nodes)
}

fn resolve_final_antiphon(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let mut nodes = section_doc(
        catalog,
        language,
        context,
        &context.maria_antiphon_source(),
        final_antiphon_section(context),
        diagnostics,
    )?;
    if context.hour == Hour::Compline {
        nodes.extend(divinum_auxilium_nodes(language));
    }
    Ok(nodes)
}

fn section_doc(
    catalog: &Catalog,
    language: &str,
    _context: &OfficeContext,
    source_key: &str,
    section: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    let nodes = section_nodes(catalog, language, source_key, section)
        .ok_or_else(|| format!("missing section `{section}` in `{source_key}`"))?;
    expand_nodes(catalog, language, &nodes, diagnostics)
}

fn first_section_doc(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    sources: &[&str],
    section: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    for source in sources {
        if let Ok(nodes) = section_doc(catalog, language, context, source, section, diagnostics) {
            return Ok(nodes);
        }
    }
    Err(format!("missing section `{section}`"))
}

fn first_of_sections(
    catalog: &Catalog,
    language: &str,
    context: &OfficeContext,
    sources: &[&str],
    sections: &[&str],
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<Vec<DocumentNode>, String> {
    for section in sections {
        if let Ok(nodes) =
            first_section_doc(catalog, language, context, sources, section, diagnostics)
        {
            return Ok(nodes);
        }
    }
    Err(format!("missing sections {sections:?}"))
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
            ContentNode::Text { text } => output.extend(semantic_text_nodes(language, text)),
            ContentNode::Rubric { text } => output.push(DocumentNode::Rubric {
                text: clean_source_text(language, text),
            }),
            ContentNode::Marker { text } => output.push(marker_node(language, text)),
            ContentNode::Heading { text } => output.push(DocumentNode::Heading {
                text: clean_source_text(language, text),
            }),
            ContentNode::Citation { text } => output.push(DocumentNode::Citation {
                text: clean_source_text(language, text),
            }),
            ContentNode::Versicle { text } => output.push(DocumentNode::Versicle {
                text: clean_source_text(language, text),
            }),
            ContentNode::Response { text } => {
                let text = clean_source_text(language, text);
                if is_amen_text(&text) {
                    output.push(DocumentNode::Amen);
                } else {
                    output.push(DocumentNode::Response { text });
                }
            }
            ContentNode::ShortResponse { text } => output.push(DocumentNode::ShortResponse {
                text: clean_source_text(language, text),
            }),
            ContentNode::Antiphon { text } => output.push(DocumentNode::Antiphon {
                text: clean_antiphon_text(language, text),
            }),
            ContentNode::Prayer { text } => output.push(DocumentNode::Prayer {
                text: clean_source_text(language, text),
            }),
            ContentNode::Blessing { text } => output.push(DocumentNode::Blessing {
                text: clean_blessing_text(language, text),
            }),
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
                    text: clean_source_text(language, label),
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

fn semantic_text_nodes(language: &str, text: &str) -> Vec<DocumentNode> {
    let mut nodes = Vec::new();
    for raw_line in text.lines() {
        let mut line = clean_source_text(language, raw_line);
        if line.trim().is_empty() {
            nodes.push(DocumentNode::Text {
                text: String::new(),
            });
            continue;
        }
        line = normalize_space_preserving_stars(&line);
        if let Some((heading, citation)) = split_parenthesized_heading_citation(&line) {
            nodes.push(DocumentNode::Heading { text: heading });
            nodes.push(DocumentNode::Citation { text: citation });
        } else if let Some(rest) = strip_role_prefix(&line, "R.br.") {
            nodes.push(DocumentNode::ShortResponse { text: rest });
        } else if let Some(rest) = strip_role_prefix(&line, "R/.") {
            push_response_node(&mut nodes, rest);
        } else if let Some(rest) = strip_role_prefix(&line, "R.") {
            push_response_node(&mut nodes, rest);
        } else if let Some(rest) = strip_role_prefix(&line, "V/.") {
            nodes.push(DocumentNode::Versicle { text: rest });
        } else if let Some(rest) = strip_role_prefix(&line, "V.") {
            nodes.push(DocumentNode::Versicle { text: rest });
        } else if let Some(rest) = strip_role_prefix(&line, "Ant.") {
            nodes.push(DocumentNode::Antiphon {
                text: clean_antiphon_text(language, &rest),
            });
        } else if let Some(rest) = strip_role_prefix(&line, "Benedictio.") {
            nodes.push(DocumentNode::Blessing {
                text: clean_blessing_text(language, &rest),
            });
        } else if let Some(rest) = strip_role_prefix(&line, "Benediction.") {
            nodes.push(DocumentNode::Blessing {
                text: clean_blessing_text(language, &rest),
            });
        } else if let Some(rest) = strip_role_prefix(&line, "v.") {
            nodes.push(classify_unprefixed_line(language, rest));
        } else if let Some(rest) = strip_role_prefix(&line, "r.") {
            nodes.push(DocumentNode::Prayer { text: rest });
        } else if looks_like_citation(&line) {
            nodes.push(DocumentNode::Citation { text: line });
        } else {
            nodes.push(classify_unprefixed_line(language, line));
        }
    }
    nodes
}

fn push_response_node(nodes: &mut Vec<DocumentNode>, text: String) {
    if is_amen_text(&text) {
        nodes.push(DocumentNode::Amen);
    } else {
        nodes.push(DocumentNode::Response { text });
    }
}

fn marker_node(language: &str, text: &str) -> DocumentNode {
    let text = clean_source_text(language, text);
    if looks_like_citation(&text) {
        DocumentNode::Citation { text }
    } else if text.to_ascii_lowercase().contains("omittitur")
        || text.eq_ignore_ascii_case("omit")
        || text.to_ascii_lowercase().starts_with("skip ")
    {
        DocumentNode::Rubric { text }
    } else {
        DocumentNode::Heading { text }
    }
}

fn classify_unprefixed_line(language: &str, line: String) -> DocumentNode {
    if looks_like_citation(&line) {
        DocumentNode::Citation { text: line }
    } else if line.eq_ignore_ascii_case("oremus.") || line.eq_ignore_ascii_case("let us pray.") {
        DocumentNode::Prayer { text: line }
    } else {
        DocumentNode::Text {
            text: clean_source_text(language, &line),
        }
    }
}

fn strip_role_prefix(line: &str, prefix: &str) -> Option<String> {
    line.strip_prefix(prefix)
        .map(str::trim)
        .filter(|rest| !rest.is_empty())
        .map(ToOwned::to_owned)
}

fn clean_antiphon_text(language: &str, text: &str) -> String {
    let text = clean_source_text(language, text);
    text.strip_prefix("Ant. ")
        .unwrap_or(&text)
        .trim()
        .to_string()
}

fn clean_blessing_text(language: &str, text: &str) -> String {
    let text = clean_source_text(language, text);
    text.strip_prefix("Benedictio.")
        .or_else(|| text.strip_prefix("Benediction."))
        .unwrap_or(&text)
        .trim()
        .to_string()
}

fn clean_source_text(_language: &str, text: &str) -> String {
    normalize_space_preserving_stars(text.trim())
}

fn normalize_space_preserving_stars(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn split_parenthesized_heading_citation(line: &str) -> Option<(String, String)> {
    let inner = line.strip_prefix('(')?.strip_suffix(')')?;
    let (heading, citation) = inner.split_once(" * ")?;
    Some((
        normalize_space_preserving_stars(heading),
        normalize_space_preserving_stars(citation),
    ))
}

fn looks_like_citation(text: &str) -> bool {
    let text = text.trim();
    if text.is_empty() {
        return false;
    }
    if text.starts_with("Psalmus ") || text.starts_with("Psalm ") {
        return false;
    }
    let has_digit = text.chars().any(|ch| ch.is_ascii_digit());
    let has_colon = text.contains(':');
    has_digit
        && (has_colon
            || text.starts_with("Ier ")
            || text.starts_with("Jer ")
            || text.starts_with("Luc. ")
            || text.starts_with("Luke ")
            || text.starts_with("1 ")
            || text.starts_with("2 ")
            || text.starts_with("3 "))
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
    if !entry.antiphon.is_empty() {
        nodes.push(DocumentNode::Antiphon {
            text: clean_antiphon_text(language, entry.antiphon.trim()),
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
            text: clean_antiphon_text(language, &close_antiphon(&entry.antiphon)),
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
        nodes.extend(formula_nodes(catalog, language, "Gloria", diagnostics).unwrap_or_default());
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

fn section_nodes(
    catalog: &Catalog,
    language: &str,
    source_key: &str,
    section: &str,
) -> Option<Vec<ContentNode>> {
    for source_key in source_key_candidates(source_key) {
        if let Some(text) = find_text_by_source_section(catalog, language, &source_key, section) {
            return Some(text.content);
        }
    }
    None
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

fn first_section_antiphons(
    catalog: &Catalog,
    language: &str,
    sources: &[&str],
    section: &str,
) -> Option<Vec<String>> {
    sources
        .iter()
        .find_map(|source| section_antiphons(catalog, language, source, section))
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

fn first_section_psalmody(
    catalog: &Catalog,
    language: &str,
    sources: &[&str],
    section: &str,
) -> Option<Vec<PsalmodyEntry>> {
    sources
        .iter()
        .find_map(|source| section_psalmody(catalog, language, source, section))
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

fn find_text_by_source_section(
    catalog: &Catalog,
    language: &str,
    source_key: &str,
    section: &str,
) -> Option<CatalogText> {
    let section_key = data_slug(section);
    let records = catalog
        .texts_by_source_key(language, source_key)
        .into_iter()
        .filter(|record| section_key_matches(record_section_key(&record.id), &section_key))
        .collect::<Vec<_>>();
    records
        .iter()
        .find(|record| record_section_key(&record.id) == section_key)
        .cloned()
        .or_else(|| records.last().cloned())
}

fn rank_from_nodes(nodes: &[ContentNode]) -> Option<RankInfo> {
    nodes.iter().find_map(|node| match node {
        ContentNode::Rank {
            label,
            value,
            common,
        } => Some(RankInfo {
            label: label.clone(),
            value: value.as_ref().map(|value| value.into_inner()),
            common: common.clone(),
        }),
        _ => None,
    })
}

#[derive(Clone, Debug)]
struct RankInfo {
    label: Option<String>,
    value: Option<f32>,
    common: Option<String>,
}

fn rule_maps(
    nodes: &[ContentNode],
) -> (BTreeSet<String>, BTreeMap<String, String>, Option<String>) {
    let mut flags = BTreeSet::new();
    let mut values = BTreeMap::new();
    let mut source = None;
    for node in nodes {
        if let ContentNode::Rule { tokens } = node {
            for token in tokens {
                match token {
                    RuleToken::Flag { id, .. } => {
                        flags.insert(normalize_rule_id(id));
                    }
                    RuleToken::Value { key, value, .. } => {
                        values.insert(normalize_rule_id(key), value.clone());
                    }
                    RuleToken::SourceRef { target, .. } => {
                        source = Some(target.clone());
                    }
                }
            }
        }
    }
    (flags, values, source)
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
    let rank = section_nodes(catalog, language, source_key, "rank")
        .and_then(|nodes| rank_from_nodes(&nodes));
    OfficeObservance {
        id,
        title,
        kind,
        rank: rank.as_ref().and_then(|rank| rank.value),
        rank_label: rank.and_then(|rank| rank.label),
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

fn commemoration_sources<'a>(
    context: &'a OfficeContext,
    commemoration: &'a CommemorationContext,
) -> Vec<&'a str> {
    let mut sources = vec![commemoration.source_key.as_str()];
    if let Some(key) = &commemoration.commune_key {
        sources.push(key.as_str());
    }
    if context
        .temporal_key
        .as_deref()
        .is_some_and(|key| key == commemoration.source_key)
        || source_key_category(&commemoration.source_key)
            .is_some_and(|category| category == "temporal")
    {
        if let Some(key) = &context.weekly_temporal_key {
            let source = key.as_str();
            if !sources.iter().any(|existing| *existing == source) {
                sources.push(source);
            }
        }
        for key in &context.collect_reference_keys {
            if source_key_category(key).is_some_and(|category| category == "temporal") {
                let source = key.as_str();
                if !sources.iter().any(|existing| *existing == source) {
                    sources.push(source);
                }
            }
        }
    }
    sources
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

fn source_key_candidates(source_key: &str) -> Vec<String> {
    vec![source_key.to_string()]
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
    let Some(reference) = source_reference_from_key(catalog, primary_language, source_key) else {
        return;
    };
    let key = source_reference_key(source_key, &reference);
    if !keys.iter().any(|existing| existing == &key) {
        keys.push(key.clone());
    }
    push_collect_reference_key(catalog, primary_language, &key, keys, seen);
}

fn source_reference_from_key(
    catalog: &Catalog,
    language: &str,
    source_key: &str,
) -> Option<String> {
    let rank_common = section_nodes(catalog, language, source_key, "rank")
        .and_then(|nodes| rank_from_nodes(&nodes).and_then(|rank| rank.common));
    let (_, _, rule_source) = section_nodes(catalog, language, source_key, "rules")
        .map(|nodes| rule_maps(&nodes))
        .unwrap_or_default();
    rule_source.or(rank_common)
}

fn source_reference_key(current_key: &str, reference: &str) -> String {
    let reference = reference.strip_suffix(".txt").unwrap_or(reference);
    if let Some(common) = reference.strip_prefix("Commune/") {
        source_key(&["common", common])
    } else if let Some(temporal) = reference.strip_prefix("Tempora/") {
        source_key(&["proper", "temporal", temporal])
    } else if let Some(sanctoral) = reference.strip_prefix("Sancti/") {
        source_key(&["proper", "sanctoral", sanctoral])
    } else if is_commune_reference(reference) {
        source_key(&["common", reference])
    } else if current_key.starts_with("proper/temporal/") {
        source_key(&["proper", "temporal", reference])
    } else if current_key.starts_with("proper/sanctoral/") {
        source_key(&["proper", "sanctoral", reference])
    } else if current_key.starts_with("common/") {
        source_key(&["common", reference])
    } else {
        data_slug(reference)
    }
}

fn is_commune_reference(reference: &str) -> bool {
    reference
        .strip_prefix('C')
        .and_then(|tail| tail.chars().next())
        .is_some_and(|ch| ch.is_ascii_digit())
}

fn source_key_category(source_key: &str) -> Option<&str> {
    if source_key.starts_with("proper/temporal/") {
        Some("temporal")
    } else if source_key.starts_with("proper/sanctoral/") {
        Some("sanctoral")
    } else {
        source_key.split('/').next()
    }
}

fn first_existing_source_key(
    catalog: &Catalog,
    language: &str,
    candidates: Vec<String>,
) -> Option<String> {
    candidates
        .into_iter()
        .find(|candidate| !catalog.texts_by_source_key(language, candidate).is_empty())
}

fn temporal_source_candidates(facts: &DateFacts) -> Vec<String> {
    vec![source_key(&["proper", "temporal", &facts.temporal_stem])]
}

fn sanctoral_source_candidates(_catalog: &Catalog, _language: &str, date_key: &str) -> Vec<String> {
    vec![source_key(&["proper", "sanctoral", date_key])]
}

fn weekly_temporal_source_candidates(facts: &DateFacts) -> Vec<String> {
    if facts.temporal_week.starts_with("Nat") {
        return Vec::new();
    }
    vec![source_key(&[
        "proper",
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
        .map(|stem| source_key(&["proper", "temporal", &stem]))
        .collect()
}

fn gregorian_easter(year: i32) -> Option<NaiveDate> {
    let golden_number = year % 19;
    let century = year / 100;
    let h = (century - century / 4 - (8 * century + 13) / 25 + 19 * golden_number + 15) % 30;
    let i = h - (h / 28) * (1 - (h / 28) * (29 / (h + 1)) * ((21 - golden_number) / 11));
    let j = (year + year / 4 + i + 2 - century + century / 4) % 7;
    let l = i - j;
    let month = 3 + (l + 40) / 44;
    let day = l + 28 - 31 * (month / 4);
    NaiveDate::from_ymd_opt(year, month as u32, day as u32)
}

fn temporal_week_key(date: NaiveDate, mass: bool) -> Result<String, DataError> {
    let year = date.year();
    let t = date.ordinal() as i32;
    let day = date.day() as i32;
    let month = date.month() as i32;
    let advent1 = advent1_ordinal(year)?;
    let christmas = ordinal_for(year, 12, 25)?;
    if t >= advent1 {
        if t < christmas {
            let n = 1 + (t - advent1) / 7;
            if month == 11 || day < 25 {
                return Ok(format!("Adv{n}"));
            }
        }
        return Ok(format!("Nat{day}"));
    }
    let ordtime = 6 + 7 - divinum_weekday_number(weekday_for(year, 1, 6)?);
    if month == 1 && t < ordtime {
        return Ok(format!("Nat{day:02}"));
    }
    let easter = gregorian_easter(year).ok_or_else(|| DataError::UnsupportedScope {
        message: format!("could not compute Easter for {year}"),
    })?;
    let easter_ordinal = easter.ordinal() as i32;
    if t < easter_ordinal - 63 {
        return Ok(format!("Epi{}", (t - ordtime) / 7 + 1));
    }
    if t < easter_ordinal - 56 {
        return Ok("Quadp1".to_string());
    }
    if t < easter_ordinal - 49 {
        return Ok("Quadp2".to_string());
    }
    if t < easter_ordinal - 42 {
        return Ok("Quadp3".to_string());
    }
    if t < easter_ordinal {
        return Ok(format!("Quad{}", 1 + (t - (easter_ordinal - 42)) / 7));
    }
    if t < easter_ordinal + 56 {
        return Ok(format!("Pasc{}", (t - easter_ordinal) / 7));
    }
    let n = (t - (easter_ordinal + 49)) / 7;
    if n < 23 {
        return Ok(format!("Pent{n:02}"));
    }
    let wdist = (advent1 - t + 6) / 7;
    if wdist < 2 {
        return Ok("Pent24".to_string());
    }
    if n == 23 {
        return Ok("Pent23".to_string());
    }
    if mass {
        Ok(format!("PentEpi{}", 8 - wdist))
    } else {
        Ok(format!("Epi{}", 8 - wdist))
    }
}

fn advent1_ordinal(year: i32) -> Result<i32, DataError> {
    let christmas =
        NaiveDate::from_ymd_opt(year, 12, 25).ok_or_else(|| DataError::UnsupportedScope {
            message: format!("could not construct Christmas for {year}"),
        })?;
    let christmas_dow = match divinum_weekday_number(christmas.weekday()) {
        0 => 7,
        day => day,
    };
    Ok(christmas.ordinal() as i32 - christmas_dow - 21)
}

fn ordinal_for(year: i32, month: u32, day: u32) -> Result<i32, DataError> {
    NaiveDate::from_ymd_opt(year, month, day)
        .map(|date| date.ordinal() as i32)
        .ok_or_else(|| DataError::UnsupportedScope {
            message: format!("could not construct {year:04}-{month:02}-{day:02}"),
        })
}

fn weekday_for(year: i32, month: u32, day: u32) -> Result<Weekday, DataError> {
    NaiveDate::from_ymd_opt(year, month, day)
        .map(|date| date.weekday())
        .ok_or_else(|| DataError::UnsupportedScope {
            message: format!("could not construct {year:04}-{month:02}-{day:02}"),
        })
}

fn sanctoral_key(date: NaiveDate) -> String {
    let month = date.month();
    let mut day = date.day();
    if is_leap_year(date.year()) && month == 2 {
        if day == 24 {
            day = 29;
        } else if day > 24 {
            day -= 1;
        }
    }
    format!("{month:02}-{day:02}")
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0) && ((year % 100 != 0) || (year % 400 == 0))
}

fn divinum_weekday_number(weekday: Weekday) -> i32 {
    weekday.num_days_from_sunday() as i32
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

fn data_slug(input: &str) -> String {
    let mut output = String::new();
    let mut last_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            output.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            output.push('-');
            last_dash = true;
        }
    }
    output.trim_matches('-').to_string()
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
        Hour::Prime => Some("Prima"),
        Hour::Terce => Some("Tertia"),
        Hour::Sext => Some("Sexta"),
        Hour::None => Some("Nona"),
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
    let sources = context.principal_sources();
    if let Some(antiphon) = canonical_minor_hour(context.hour)
        .and_then(|canonical_hour| {
            first_section_antiphons(
                catalog,
                language,
                &sources,
                &format!("{canonical_hour}-antiphon"),
            )
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
            first_section_antiphons(catalog, language, &sources, section)
                .and_then(|values| values.get(index).cloned())
                .filter(|value| !value.trim().is_empty())
        })
}

fn first_nonempty_antiphon(values: Vec<String>) -> Option<String> {
    values.into_iter().find(|value| !value.trim().is_empty())
}

fn minor_special_season(context: &OfficeContext) -> &'static str {
    if context.facts.temporal_week.starts_with("Adv") {
        "Adv"
    } else if context.facts.temporal_week.starts_with("Quad5") {
        "Quad5"
    } else if context.facts.temporal_week.starts_with("Quad") {
        "Quad"
    } else if context.facts.temporal_week.starts_with("Pasc") {
        "Pasch"
    } else if context.facts.weekday == Weekday::Sun {
        "Dominica"
    } else {
        "Feria"
    }
}

fn prime_season(context: &OfficeContext) -> &'static str {
    if context.facts.temporal_week.starts_with("Adv") {
        "Adv"
    } else if context.facts.temporal_week.starts_with("Nat") {
        "Nat"
    } else if context.facts.temporal_week.starts_with("Epi") {
        "Epi"
    } else if context.facts.temporal_week.starts_with("Quad5") {
        "Quad5"
    } else if context.facts.temporal_week.starts_with("Quad") {
        "Quad"
    } else if context.facts.temporal_week.starts_with("Pasc") {
        "Pasch"
    } else {
        "Per Annum"
    }
}

fn matins_ordinary_hymn_section(context: &OfficeContext) -> String {
    if context.facts.temporal_week.starts_with("Adv") {
        "Hymnus Adv".to_string()
    } else if context.facts.temporal_week.starts_with("Quad") {
        "Hymnus Quad".to_string()
    } else if context.facts.temporal_week.starts_with("Pasc") {
        "Hymnus Pasch".to_string()
    } else {
        format!(
            "Day{} Hymnus",
            divinum_weekday_number(context.facts.weekday)
        )
    }
}

fn ferial_benedictus_antiphon_section(context: &OfficeContext) -> String {
    let weekday = divinum_weekday_number(context.facts.weekday);
    if weekday == 0 {
        "Dominica Ant 2".to_string()
    } else {
        format!("Feria{} Ant 2", weekday + 1)
    }
}

fn ferial_magnificat_antiphon_section(context: &OfficeContext) -> String {
    let weekday = divinum_weekday_number(context.facts.weekday);
    if weekday == 0 {
        "Dominica Ant 3".to_string()
    } else {
        format!("Feria{} Ant 3", weekday + 1)
    }
}

fn compline_antiphon_section(context: &OfficeContext) -> &'static str {
    if context.facts.temporal_week.starts_with("Quad5") {
        "compline-gospel-antiphon-passiontide"
    } else if context.facts.temporal_week.starts_with("Quad") {
        "compline-gospel-antiphon-lent"
    } else if context.facts.temporal_week.starts_with("Pasc") {
        "compline-gospel-antiphon-easter"
    } else {
        "compline-gospel-antiphon"
    }
}

fn final_antiphon_section(context: &OfficeContext) -> &'static str {
    let date = context.facts.date;
    if context.facts.temporal_week.starts_with("Adv") {
        "Advent"
    } else if date.month() == 12 && date.day() >= 25 || date.month() == 1 {
        "Nativiti"
    } else if date >= context.facts.easter && date <= context.facts.easter + Duration::days(56) {
        "Paschalis"
    } else if context.facts.temporal_week.starts_with("Quad") {
        "Quadragesimae"
    } else {
        "Postpentecost"
    }
}

fn builtin_steps(hour: Hour) -> Vec<RawOfficeStep> {
    match hour {
        Hour::Matins => vec![
            step(
                "opening",
                TextRole::Opening,
                OfficeStepKind::MatinsOpening,
                "Incipit",
                "Start",
            ),
            step(
                "invitatory",
                TextRole::Invitatory,
                OfficeStepKind::MatinsInvitatory,
                "Invitatorium",
                "Invitatory",
            ),
            step(
                "hymn",
                TextRole::Hymn,
                OfficeStepKind::MatinsHymn,
                "Hymnus",
                "Hymn",
            ),
            step(
                "nocturns",
                TextRole::Reading,
                OfficeStepKind::MatinsNocturns,
                "Nocturni",
                "Nocturns",
            ),
        ],
        Hour::Lauds => vec![
            step(
                "opening",
                TextRole::Opening,
                OfficeStepKind::Opening,
                "Incipit",
                "Start",
            ),
            step(
                "psalmody",
                TextRole::Psalmody,
                OfficeStepKind::LaudsPsalmody,
                "Psalmi",
                "Psalms",
            ),
            step(
                "chapter",
                TextRole::Chapter,
                OfficeStepKind::MajorChapterHymnVerse,
                "Capitulum Hymnus Versus",
                "Chapter Hymn Verse",
            ),
            step(
                "benedictus",
                TextRole::GospelCanticle,
                OfficeStepKind::GospelCanticle,
                "Canticum: Benedictus",
                "Canticle: Benedictus",
            ),
            step(
                "preces",
                TextRole::Preces,
                OfficeStepKind::Preces,
                "Preces",
                "Preces",
            ),
            step(
                "collect",
                TextRole::Collect,
                OfficeStepKind::Collects,
                "Oratio",
                "Prayer",
            ),
            step(
                "conclusion",
                TextRole::Conclusion,
                OfficeStepKind::Conclusion,
                "Conclusio",
                "Conclusion",
            ),
        ],
        Hour::Prime => vec![
            step(
                "opening",
                TextRole::Opening,
                OfficeStepKind::Opening,
                "Incipit",
                "Start",
            ),
            step(
                "hymn",
                TextRole::Hymn,
                OfficeStepKind::PrimeHymn,
                "Hymnus",
                "Hymn",
            ),
            step(
                "psalmody",
                TextRole::Psalmody,
                OfficeStepKind::MinorPsalmody,
                "Psalmi",
                "Psalms",
            ),
            step(
                "chapter",
                TextRole::Chapter,
                OfficeStepKind::MinorChapterResponsoryVerse,
                "Capitulum",
                "Chapter",
            ),
            step(
                "collect",
                TextRole::Collect,
                OfficeStepKind::PrimeCollect,
                "Oratio",
                "Prayer",
            ),
            step(
                "martyrology",
                TextRole::MartyrologyEntry,
                OfficeStepKind::PrimeMartyrology,
                "Martyrologium",
                "Martyrology",
            ),
            step(
                "pretiosa",
                TextRole::Versicle,
                OfficeStepKind::PrimePretiosa,
                "Pretiosa",
                "Pretiosa",
            ),
            step(
                "chapter-office",
                TextRole::Chapter,
                OfficeStepKind::PrimeChapterOffice,
                "Capitulum",
                "Chapter Office",
            ),
            step(
                "short-reading",
                TextRole::ShortReading,
                OfficeStepKind::PrimeShortReading,
                "Lectio brevis",
                "Short Reading",
            ),
            step(
                "conclusion",
                TextRole::Conclusion,
                OfficeStepKind::PrimeConclusion,
                "Conclusio",
                "Conclusion",
            ),
        ],
        Hour::Terce | Hour::Sext | Hour::None => vec![
            step(
                "opening",
                TextRole::Opening,
                OfficeStepKind::Opening,
                "Incipit",
                "Start",
            ),
            step(
                "hymn",
                TextRole::Hymn,
                OfficeStepKind::MinorHymn,
                "Hymnus",
                "Hymn",
            ),
            step(
                "psalmody",
                TextRole::Psalmody,
                OfficeStepKind::MinorPsalmody,
                "Psalmi",
                "Psalms",
            ),
            step(
                "chapter",
                TextRole::Chapter,
                OfficeStepKind::MinorChapterResponsoryVerse,
                "Capitulum",
                "Chapter",
            ),
            step(
                "preces",
                TextRole::Preces,
                OfficeStepKind::Preces,
                "Preces",
                "Preces",
            ),
            step(
                "collect",
                TextRole::Collect,
                OfficeStepKind::Collects,
                "Oratio",
                "Prayer",
            ),
            step(
                "conclusion",
                TextRole::Conclusion,
                OfficeStepKind::Conclusion,
                "Conclusio",
                "Conclusion",
            ),
        ],
        Hour::Vespers => vec![
            step(
                "opening",
                TextRole::Opening,
                OfficeStepKind::Opening,
                "Incipit",
                "Start",
            ),
            step(
                "psalmody",
                TextRole::Psalmody,
                OfficeStepKind::VespersPsalmody,
                "Psalmi",
                "Psalms",
            ),
            step(
                "chapter",
                TextRole::Chapter,
                OfficeStepKind::VespersChapterHymnVerse,
                "Capitulum Hymnus Versus",
                "Chapter Hymn Verse",
            ),
            step(
                "magnificat",
                TextRole::GospelCanticle,
                OfficeStepKind::Magnificat,
                "Canticum: Magnificat",
                "Canticle: Magnificat",
            ),
            step(
                "preces",
                TextRole::Preces,
                OfficeStepKind::Preces,
                "Preces",
                "Preces",
            ),
            step(
                "collect",
                TextRole::Collect,
                OfficeStepKind::Collects,
                "Oratio",
                "Prayer",
            ),
            step(
                "conclusion",
                TextRole::Conclusion,
                OfficeStepKind::Conclusion,
                "Conclusio",
                "Conclusion",
            ),
        ],
        Hour::Compline => vec![
            step(
                "opening",
                TextRole::Opening,
                OfficeStepKind::ComplineOpening,
                "Benedictio",
                "Blessing",
            ),
            step(
                "short-reading",
                TextRole::ShortReading,
                OfficeStepKind::ComplineShortReading,
                "Lectio brevis",
                "Short Reading",
            ),
            step(
                "examination",
                TextRole::Preces,
                OfficeStepKind::ComplineExamination,
                "Examen",
                "Examination",
            ),
            step(
                "opening-2",
                TextRole::Opening,
                OfficeStepKind::Opening,
                "Incipit",
                "Start",
            ),
            step(
                "psalmody",
                TextRole::Psalmody,
                OfficeStepKind::ComplinePsalmody,
                "Psalmi",
                "Psalms",
            ),
            step(
                "hymn",
                TextRole::Hymn,
                OfficeStepKind::ComplineHymn,
                "Hymnus",
                "Hymn",
            ),
            step(
                "chapter",
                TextRole::Chapter,
                OfficeStepKind::ComplineChapterResponsoryVerse,
                "Capitulum",
                "Chapter",
            ),
            step(
                "nunc-dimittis",
                TextRole::GospelCanticle,
                OfficeStepKind::NuncDimittis,
                "Canticum: Nunc dimittis",
                "Canticle: Nunc dimittis",
            ),
            step(
                "collect",
                TextRole::Collect,
                OfficeStepKind::ComplineCollect,
                "Oratio",
                "Prayer",
            ),
            step(
                "conclusion",
                TextRole::Conclusion,
                OfficeStepKind::ComplineConclusion,
                "Conclusio",
                "Conclusion",
            ),
            step(
                "final-antiphon",
                TextRole::MarianAntiphon,
                OfficeStepKind::FinalAntiphon,
                "Antiphona finalis",
                "Final Antiphon",
            ),
        ],
    }
}

fn step(
    id: &str,
    role: TextRole,
    kind: OfficeStepKind,
    latin: &str,
    english: &str,
) -> RawOfficeStep {
    RawOfficeStep {
        id: id.to_string(),
        role,
        kind,
        titles: BTreeMap::from([
            ("la".to_string(), latin.to_string()),
            ("en".to_string(), english.to_string()),
        ]),
    }
}

#[derive(Default)]
struct CatalogBuilder {
    profiles: BTreeMap<ProfileId, RawProfile>,
    rites: BTreeMap<RecordId, RawRite>,
    skeletons: BTreeMap<(ProfileId, Hour), RawOfficeSkeleton>,
    corpus: BTreeMap<RecordId, RawCorpusRecord>,
    sources: BTreeMap<String, RawSource>,
}

impl CatalogBuilder {
    fn insert(&mut self, path: String, document: RawDocument) -> Result<(), DataError> {
        match document {
            RawDocument::Profile(profile) => {
                insert_unique(&mut self.profiles, profile.id.clone(), profile, &path)
            }
            RawDocument::Rite(rite) => insert_unique(&mut self.rites, rite.id.clone(), rite, &path),
            RawDocument::OfficeSkeleton(skeleton) => insert_unique(
                &mut self.skeletons,
                (skeleton.profile.clone(), skeleton.hour),
                skeleton,
                &path,
            ),
            RawDocument::CorpusBundle(bundle) => {
                for (id, record) in bundle.texts {
                    insert_unique(&mut self.corpus, id, record, &path)?;
                }
                Ok(())
            }
            RawDocument::SourceBundle(bundle) => {
                for (key, source) in bundle.sources {
                    insert_unique(&mut self.sources, key, source, &path)?;
                }
                Ok(())
            }
        }
    }

    fn finish(self) -> Result<Catalog, DataError> {
        let texts = build_text_index(&self.profiles, &self.corpus, &self.sources)?;
        let mut catalog = Catalog {
            profiles: self.profiles,
            rites: self.rites,
            skeletons: self.skeletons,
            corpus: self.corpus,
            texts,
            source_key_index: BTreeMap::new(),
        };
        catalog.source_key_index = build_source_key_index(&catalog.texts);
        validate_catalog(&catalog)?;
        Ok(catalog)
    }
}

fn build_source_key_index(
    texts: &BTreeMap<(LanguageId, RecordId), RawTextRecord>,
) -> BTreeMap<(LanguageId, String), Vec<RecordId>> {
    let mut index = BTreeMap::<(LanguageId, String), Vec<RecordId>>::new();
    for (language, id) in texts.keys() {
        let Some(source_key) = source_key_from_record_id(id) else {
            continue;
        };
        index
            .entry((language.clone(), source_key))
            .or_default()
            .push(id.clone());
    }
    index
}

fn build_text_index(
    profiles: &BTreeMap<ProfileId, RawProfile>,
    corpus: &BTreeMap<RecordId, RawCorpusRecord>,
    sources: &BTreeMap<String, RawSource>,
) -> Result<BTreeMap<(LanguageId, RecordId), RawTextRecord>, DataError> {
    let supported_languages = profiles
        .values()
        .flat_map(|profile| profile.supported_languages.iter().cloned())
        .collect::<BTreeSet<_>>();
    let metadata_languages = if supported_languages.is_empty() {
        BTreeSet::from(["la".to_string(), "en".to_string()])
    } else {
        supported_languages
    };
    let mut texts = BTreeMap::new();

    for (source_key, source) in sources {
        let source_id = source_key.replace('/', ".");
        if let Some(rank) = &source.metadata.rank {
            let id = format!("{source_id}.rank");
            for language in &metadata_languages {
                insert_unique(
                    &mut texts,
                    (language.clone(), id.clone()),
                    RawTextRecord {
                        role: TextRole::Rubric,
                        content: vec![ContentNode::Rank {
                            label: rank.label.clone(),
                            value: rank.value,
                            common: rank.common.clone(),
                        }],
                    },
                    source_key,
                )?;
            }
        }
        if !source.metadata.rules.is_empty() {
            let id = format!("{source_id}.rules");
            for language in &metadata_languages {
                insert_unique(
                    &mut texts,
                    (language.clone(), id.clone()),
                    RawTextRecord {
                        role: TextRole::Rubric,
                        content: vec![ContentNode::Rule {
                            tokens: source.metadata.rules.clone(),
                        }],
                    },
                    source_key,
                )?;
            }
        }

        for (section_key, section) in &source.sections {
            let record = corpus.get(&section.text_id).ok_or_else(|| {
                DataError::InvalidCatalog {
                    message: format!(
                        "source `{source_key}` section `{section_key}` references unknown corpus text `{}`",
                        section.text_id
                    ),
                }
            })?;
            let id = format!("{source_id}.{section_key}");
            for (language, content) in &record.content {
                insert_unique(
                    &mut texts,
                    (language.clone(), id.clone()),
                    RawTextRecord {
                        role: section.role.clone(),
                        content: content.clone(),
                    },
                    source_key,
                )?;
            }
        }
    }

    Ok(texts)
}

fn source_key_from_record_id(id: &str) -> Option<String> {
    let (source, _) = id.rsplit_once('.')?;
    Some(source.replace('.', "/"))
}

fn record_section_key(id: &str) -> &str {
    id.rsplit_once('.')
        .map(|(_, section)| section)
        .unwrap_or(id)
}

fn section_key_matches(record_key: &str, query_key: &str) -> bool {
    record_key == query_key
        || record_key
            .strip_prefix(query_key)
            .and_then(|tail| tail.strip_prefix('-'))
            .is_some_and(|tail| tail.chars().all(|ch| ch.is_ascii_digit()))
}

fn insert_unique<K, V>(
    map: &mut BTreeMap<K, V>,
    key: K,
    value: V,
    path: &str,
) -> Result<(), DataError>
where
    K: Ord + std::fmt::Debug,
{
    if map.insert(key, value).is_some() {
        return Err(DataError::InvalidCatalog {
            message: format!("duplicate ID while loading {path}"),
        });
    }
    Ok(())
}

fn validate_catalog(catalog: &Catalog) -> Result<(), DataError> {
    for profile in catalog.profiles.values() {
        if !catalog.rites.contains_key(&profile.rite) {
            return Err(DataError::InvalidCatalog {
                message: format!(
                    "profile `{}` references unknown rite `{}`",
                    profile.id, profile.rite
                ),
            });
        }
    }
    Ok(())
}

#[allow(dead_code)]
#[derive(Deserialize)]
#[serde(tag = "doc_type", rename_all = "snake_case")]
enum RawDocument {
    Profile(RawProfile),
    Rite(RawRite),
    OfficeSkeleton(RawOfficeSkeleton),
    CorpusBundle(RawCorpusBundle),
    SourceBundle(RawSourceBundle),
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
struct RawProfile {
    id: String,
    title: String,
    rite: String,
    supported_services: Vec<String>,
    supported_languages: Vec<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
struct RawRite {
    id: String,
    title: String,
    hours: Vec<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
struct RawOfficeSkeleton {
    id: String,
    profile: ProfileId,
    hour: Hour,
    steps: Vec<RawOfficeStep>,
}

#[derive(Clone, Debug, Deserialize)]
struct RawOfficeStep {
    id: String,
    role: TextRole,
    kind: OfficeStepKind,
    titles: BTreeMap<LanguageId, String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum OfficeStepKind {
    Opening,
    MatinsOpening,
    MatinsInvitatory,
    MatinsHymn,
    MatinsNocturns,
    LaudsPsalmody,
    MajorChapterHymnVerse,
    GospelCanticle,
    VespersPsalmody,
    VespersChapterHymnVerse,
    Magnificat,
    PrimeHymn,
    MinorHymn,
    MinorPsalmody,
    MinorChapterResponsoryVerse,
    PrimeCollect,
    PrimeMartyrology,
    PrimePretiosa,
    PrimeChapterOffice,
    PrimeShortReading,
    PrimeConclusion,
    ComplineOpening,
    ComplineShortReading,
    ComplineExamination,
    ComplinePsalmody,
    ComplineHymn,
    ComplineChapterResponsoryVerse,
    NuncDimittis,
    ComplineCollect,
    ComplineConclusion,
    Preces,
    Collects,
    Conclusion,
    FinalAntiphon,
    Unsupported,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
struct RawCorpusBundle {
    texts: BTreeMap<String, RawCorpusRecord>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
struct RawCorpusRecord {
    role: TextRole,
    content: BTreeMap<LanguageId, Vec<ContentNode>>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
struct RawSourceBundle {
    sources: BTreeMap<String, RawSource>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Default, Deserialize)]
struct RawSource {
    #[serde(default)]
    metadata: RawSourceMetadata,
    #[serde(default)]
    sections: BTreeMap<String, RawSourceSection>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Default, Deserialize)]
struct RawSourceMetadata {
    #[serde(default)]
    rank: Option<RawRankMetadata>,
    #[serde(default)]
    rules: Vec<RuleToken>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
struct RawRankMetadata {
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    value: Option<OrderedFloat<f32>>,
    #[serde(default)]
    common: Option<String>,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
struct RawSourceSection {
    role: TextRole,
    text_id: RecordId,
}

#[allow(dead_code)]
#[derive(Clone, Debug, Deserialize)]
struct RawTextRecord {
    role: TextRole,
    content: Vec<ContentNode>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_facts_are_available() {
        let facts = office_date_facts(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()).unwrap();
        assert_eq!(facts.sanctoral_key, "01-01");
    }
}
