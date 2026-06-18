//! The books + lexicon on-disk schema — the single contract between the
//! importer (which serializes this shape) and the runtime loader (which
//! deserializes it). Replaces the old `corpus_bundle` / `source_bundle`
//! documents.
//!
//! Layout under `data/`:
//!
//! ```text
//! data/
//!   books/                 # the books the resolver stacks
//!     temporal.yaml        # Proprium de Tempore
//!     sanctoral.yaml       # Proprium Sanctorum
//!     commons.yaml         # Commune Sanctorum
//!     psalter.yaml         # Psalterium (ferial psalms/antiphons by day+hour)
//!     ordinary.yaml        # Ordinarium (fixed formulae)
//!     martyrology.yaml
//!     psalms.yaml          # psalm/canticle texts, referenced by psalmody
//!   lexicon/               # deduped multilingual texts, referenced by id
//!     *.yaml
//!   rite.yaml
//!   profile.yaml
//! ```
//!
//! A book file is a map of office key → [`RawOffice`]. The book itself is named
//! by its file stem (`temporal`, `sanctoral`, …); office keys are book-relative
//! (`Nat01`, `01-01`, `c4`, `sunday`). Slots are canonical names ([`crate::slots`])
//! mapping to lexicon text ids.

use crate::{ContentNode, LanguageId, RecordId, TextRole};
use ordered_float::OrderedFloat;
use serde::Deserialize;
use std::collections::BTreeMap;

/// One book file: a map of book-relative office key → office.
#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct BookFile {
    #[serde(default)]
    pub offices: BTreeMap<String, RawOffice>,
}

/// One observance / structural unit within a book.
#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct RawOffice {
    /// Precedence rank (absent for the psalter/ordinary structural offices).
    #[serde(default)]
    pub rank: Option<RawRank>,
    /// Normalized rubrical flag ids (`psalmi-dominica`, `9-lectiones`, …).
    #[serde(default)]
    pub flags: Vec<String>,
    /// Rubrical key/value rules (`laudes` → `2`, …).
    #[serde(default)]
    pub values: BTreeMap<String, String>,
    /// Cross-book reference to the common this office draws on, as a
    /// `book/office` key (e.g. `commons/c4`). Resolved by the importer.
    #[serde(default)]
    pub common: Option<String>,
    /// Canonical slot name → lexicon text id.
    #[serde(default)]
    pub slots: BTreeMap<String, RecordId>,
}

/// Precedence rank of an observance.
#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct RawRank {
    /// Human-readable rank name (`Duplex I classis`).
    #[serde(default)]
    pub name: Option<String>,
    /// Numeric precedence value used for occurrence resolution.
    #[serde(default)]
    pub value: Option<OrderedFloat<f32>>,
}

/// One lexicon file: a map of text id → multilingual entry.
#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct LexiconFile {
    #[serde(default)]
    pub texts: BTreeMap<String, RawLexEntry>,
}

/// A reusable, multilingual text unit referenced by book slots.
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct RawLexEntry {
    /// Semantic role (`antiphon`, `collect`, `hymn`, `reading`, …).
    pub role: TextRole,
    /// Per-language content nodes.
    #[serde(default)]
    pub content: BTreeMap<LanguageId, Vec<ContentNode>>,
}

/// Resolver-emitted localized phrases (versicle/response formulae, structural
/// titles, inline rubric words) keyed by a stable id, one column per language.
/// Keeps these strings out of code so any language overrides them via data.
#[derive(Clone, Debug, Default, Deserialize)]
pub(crate) struct PhrasesFile {
    #[serde(default)]
    pub phrases: BTreeMap<String, BTreeMap<LanguageId, String>>,
}

/// Rubrical profile (`roman-1960`).
#[derive(Clone, Debug, Deserialize)]
pub(crate) struct RawProfile {
    pub id: String,
    #[serde(default)]
    pub supported_languages: Vec<String>,
}
