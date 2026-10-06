/// A selectable vernacular column. Single source of truth for the language's
/// URL path segment and its display label, mirroring [`crate::officium::hour`].
///
/// Latin is deliberately absent: [`crate::officium::Officium`] always renders
/// Latin in the left column, so offering it here would only duplicate that
/// column. The `la` segment stays resolvable for hand-written URLs — see
/// [`is_supported_code`] — it is simply never linked to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Language {
    English,
    DouayRheims,
    Chinese,
    ChineseEcumenical,
}

/// The `localStorage` key holding the reader's language choice. Namespaced
/// because `localStorage` is shared across the whole origin.
pub(crate) const STORAGE_KEY: &str = "breviarium.lang";

impl Language {
    /// Every selectable language, in the order the pickers render them.
    pub(crate) const ALL: [Language; 4] = [
        Language::English,
        Language::DouayRheims,
        Language::Chinese,
        Language::ChineseEcumenical,
    ];

    /// The language assumed before the reader has chosen one.
    pub(crate) const DEFAULT: Language = Language::English;

    /// Resolves a language from a URL path segment or a stored preference.
    /// Returns `None` for `la`, which is resolvable but not selectable.
    pub(crate) fn from_code(code: &str) -> Option<Language> {
        Some(match code {
            "en" => Language::English,
            "en-dr" => Language::DouayRheims,
            "zhs" => Language::Chinese,
            "zhs-ecu" => Language::ChineseEcumenical,
            _ => return None,
        })
    }

    /// The URL path segment, matching the lexicon column name.
    pub(crate) fn code(self) -> &'static str {
        match self {
            Language::English => "en",
            Language::DouayRheims => "en-dr",
            Language::Chinese => "zhs",
            Language::ChineseEcumenical => "zhs-ecu",
        }
    }

    /// The label shown in the pickers.
    ///
    /// The ecumenical Chinese column is labelled 普世 rather than 和合本: it is
    /// not strictly the 和合本 text, since passages carry emendations aligning
    /// them with the Latin and 耶和华 is avoided.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Language::English => "English",
            Language::DouayRheims => "English (D-R)",
            Language::Chinese => "简体中文",
            Language::ChineseEcumenical => "简体中文（普世）",
        }
    }
}

/// Whether the resolver can serve this language column. Broader than
/// [`Language::from_code`] because Latin is resolvable without being
/// selectable, so an existing `/la/...` URL keeps working.
pub(crate) fn is_supported_code(code: &str) -> bool {
    code == LATIN || Language::from_code(code).is_some()
}

/// The language rendered in the fixed left column of every Office page.
pub(crate) const LATIN: &str = "la";

/// Records the reader's language choice so a later visit to the home page can
/// restore it. Called only on a deliberate selection in the Office header
/// switcher — never on a mere page visit, so following someone
/// else's link cannot silently rewrite the preference.
///
/// `localStorage` throws in some private-browsing modes; a preference that
/// fails to save must not break the page, hence the `try`.
pub(crate) fn remember(language: Language) {
    // Latin is never selectable, so it can never reach here as a choice.
    let script = format!(
        "try {{ localStorage.setItem('{STORAGE_KEY}', '{}'); }} catch (e) {{}}",
        language.code()
    );
    dioxus::document::eval(&script);
}

#[cfg(test)]
mod tests {
    use super::{is_supported_code, Language, LATIN};

    #[test]
    fn from_code_round_trips_every_selectable_language() {
        for language in Language::ALL {
            assert_eq!(Language::from_code(language.code()), Some(language));
        }
    }

    #[test]
    fn latin_resolves_but_is_not_selectable() {
        assert_eq!(Language::from_code(LATIN), None);
        assert!(is_supported_code(LATIN));
    }

    #[test]
    fn unknown_codes_are_rejected() {
        for code in ["", "xx", "EN", "zh", "zhs-", "en_dr"] {
            assert_eq!(Language::from_code(code), None);
            assert!(!is_supported_code(code));
        }
    }
}
