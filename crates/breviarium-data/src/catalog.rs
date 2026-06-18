//! The books + lexicon data layer.
//!
//! Replaces the old corpus/source loader. A [`Catalog`] holds the liturgical
//! *books* (temporal, sanctoral, commons, psalter, ordinary, martyrology, psalm)
//! and a shared *lexicon* of multilingual texts. Source keys are `book/office`
//! (e.g. `sanctoral/01-01`, `ordinary/formulae`, `psalm/94`); a slot lookup
//! follows `office.slots[slot] → lexicon[id].content[language]`.
//!
//! The resolver consults books as an ordered *stack* (proper → common → temporal
//! → psalter → ordinary); see `resolve`. The leaf text primitives in `content`
//! reach the data only through [`section_nodes`], so swapping this layer left
//! them untouched.

use crate::schema::{BookFile, LexiconFile, PhrasesFile, RawProfile};
use crate::{ContentNode, DataError, TextRole, DATA_DIR};
use std::collections::{BTreeMap, BTreeSet};

/// One observance / structural unit within a book.
#[derive(Clone, Debug, Default)]
pub(crate) struct Office {
    /// Numeric precedence rank (absent for psalter/ordinary structural offices).
    pub rank: Option<f32>,
    /// Human-readable rank name.
    pub rank_name: Option<String>,
    /// Normalized rubrical flag ids.
    pub flags: BTreeSet<String>,
    /// Rubrical key/value rules.
    pub values: BTreeMap<String, String>,
    /// Raw cross-book common reference (resolved to a `book/office` key by the
    /// resolver via `source_reference_key`).
    pub common: Option<String>,
    /// Canonical slot name → lexicon text id.
    pub slots: BTreeMap<String, String>,
}

#[derive(Clone, Debug)]
struct LexEntry {
    role: TextRole,
    content: BTreeMap<String, Vec<ContentNode>>,
}

/// A reusable multilingual lexicon text, exposed for translation export.
#[derive(Clone, Debug)]
pub struct CorpusText {
    /// Lexicon text id.
    pub id: String,
    /// Semantic role.
    pub role: TextRole,
    /// Per-language content nodes.
    pub content: BTreeMap<String, Vec<ContentNode>>,
}

/// A rubrical profile (`roman-1960`).
#[derive(Clone, Debug)]
pub struct Profile {
    /// Profile id.
    pub id: String,
    /// Languages this profile ships text for.
    pub supported_languages: Vec<String>,
}

/// Immutable embedded catalog: books + shared lexicon + profiles.
#[derive(Clone, Debug, Default)]
pub struct Catalog {
    books: BTreeMap<String, BTreeMap<String, Office>>,
    lexicon: BTreeMap<String, LexEntry>,
    profiles: BTreeMap<String, Profile>,
    /// Resolver-emitted localized phrases: id → language → text.
    phrases: BTreeMap<String, BTreeMap<String, String>>,
}

impl Catalog {
    /// Looks up an office by `book/office` source key.
    pub(crate) fn office(&self, source_key: &str) -> Option<&Office> {
        let (book, office) = source_key.split_once('/')?;
        self.books.get(book)?.get(office)
    }

    /// True if the `book/office` exists.
    pub(crate) fn has_office(&self, source_key: &str) -> bool {
        self.office(source_key).is_some()
    }

    /// Returns the content nodes filling `slot` of `source_key` in `language`.
    /// Slot names are matched exactly: the importer emits canonical kebab keys
    /// and the resolver requests them verbatim — no runtime normalization.
    pub(crate) fn slot_nodes(
        &self,
        language: &str,
        source_key: &str,
        slot: &str,
    ) -> Option<Vec<ContentNode>> {
        let id = self.office(source_key)?.slots.get(slot)?;
        self.lexicon.get(id)?.content.get(language).cloned()
    }

    /// Looks up a rubrical profile.
    pub fn profile(&self, id: &str) -> Option<&Profile> {
        self.profiles.get(id)
    }

    /// Returns the localized `phrase` for `language`, falling back to the Latin
    /// (`la`) column when the language is absent, and to the id itself when the
    /// phrase is undefined (a visible signal of a missing entry).
    pub(crate) fn phrase<'a>(&'a self, language: &str, id: &'a str) -> &'a str {
        let by_lang = self.phrases.get(id);
        by_lang
            .and_then(|m| m.get(language).or_else(|| m.get("la")))
            .map(String::as_str)
            .unwrap_or(id)
    }

    /// Iterates the reusable lexicon texts (used by translation export).
    pub fn corpus_texts(&self) -> impl Iterator<Item = CorpusText> + '_ {
        self.lexicon.iter().map(|(id, entry)| CorpusText {
            id: id.clone(),
            role: entry.role.clone(),
            content: entry.content.clone(),
        })
    }
}

/// The single data-access primitive the leaf text helpers depend on: the content
/// nodes filling `section` (a canonical slot) of `source_key` (`book/office`).
pub(crate) fn section_nodes(
    catalog: &Catalog,
    language: &str,
    source_key: &str,
    section: &str,
) -> Option<Vec<ContentNode>> {
    catalog.slot_nodes(language, source_key, section)
}

/// Loads the embedded books + lexicon + profiles.
pub(crate) fn load_catalog() -> Result<Catalog, DataError> {
    let mut catalog = Catalog::default();
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
        if let Some(name) = path
            .strip_prefix("books/")
            .and_then(|p| p.strip_suffix(".yaml"))
        {
            let file: BookFile = crate::load_yaml(&path)?;
            let offices = file
                .offices
                .into_iter()
                .map(|(key, office)| (key, office_from_schema(office)))
                .collect();
            catalog.books.insert(name.to_string(), offices);
        } else if path.starts_with("lexicon/") {
            let file: LexiconFile = crate::load_yaml(&path)?;
            for (id, entry) in file.texts {
                catalog.lexicon.insert(
                    id,
                    LexEntry {
                        role: entry.role,
                        content: entry.content,
                    },
                );
            }
        } else if path.starts_with("profiles/") {
            let raw: RawProfile = crate::load_yaml(&path)?;
            catalog.profiles.insert(
                raw.id.clone(),
                Profile {
                    id: raw.id,
                    supported_languages: raw.supported_languages,
                },
            );
        } else if path == "phrases.yaml" {
            let file: PhrasesFile = crate::load_yaml(&path)?;
            catalog.phrases = file.phrases;
        }
        // rites/ and the legacy corpus/+sources/ are ignored.
    }

    if catalog.books.is_empty() {
        return Err(DataError::InvalidCatalog {
            message: "no books found under data/books".to_string(),
        });
    }
    Ok(catalog)
}

fn office_from_schema(raw: crate::schema::RawOffice) -> Office {
    Office {
        rank: raw
            .rank
            .as_ref()
            .and_then(|r| r.value.map(|v| v.into_inner())),
        rank_name: raw.rank.and_then(|r| r.name),
        flags: raw.flags.into_iter().collect(),
        values: raw.values,
        common: raw.common,
        slots: raw.slots,
    }
}
