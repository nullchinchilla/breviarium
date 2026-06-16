//! Public data model: the typed liturgical content and Office result types
//! returned by the resolver. Pure data — no catalog or resolution logic.

use chrono::{NaiveDate, Weekday};
use ordered_float::OrderedFloat;
use serde::Deserialize;
use thiserror::Error;

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
    /// Conclusion.
    Conclusion,
    /// Final Marian antiphon.
    MarianAntiphon,
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
    pub(crate) fn label(&self) -> &str {
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
    pub(crate) catalog_key: Option<String>,
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
