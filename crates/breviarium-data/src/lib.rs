#![deny(missing_docs)]
//! Embedded structured liturgical data and Office resolution.
//!
//! `breviarium-data` embeds YAML data at compile time and exposes a typed API
//! for resolving Office hours. The YAML format is normalized into two layers:
//! reusable multilingual corpus texts, and liturgical source sections that
//! reference those corpus texts by ID. The corpus is fully expanded ahead of
//! time; runtime code sees ordinary records such as antiphons, psalm
//! references, rank metadata, and rule tokens.
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

use include_dir::{include_dir, Dir};
use serde::Deserialize;
use std::sync::OnceLock;

pub mod slots;

mod calendar;
mod catalog;
mod model;
mod resolve;
mod schema;

pub use calendar::office_date_facts;
pub use catalog::{Catalog, CorpusText, Profile};
pub use model::*;

static DATA_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/data");
static CATALOG: OnceLock<Result<Catalog, DataError>> = OnceLock::new();

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
        resolve::resolve_office(self.catalog, request)
    }
}

/// Returns the lazily parsed embedded catalog.
pub fn catalog() -> Result<&'static Catalog, DataError> {
    CATALOG
        .get_or_init(catalog::load_catalog)
        .as_ref()
        .map_err(Clone::clone)
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

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn date_facts_are_available() {
        let facts = office_date_facts(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap()).unwrap();
        assert_eq!(facts.sanctoral_key, "01-01");
    }
}
