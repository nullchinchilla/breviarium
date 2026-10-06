/// A canonical office hour. Single source of truth for the hour's URL path
/// segment, its display label, and the aliases accepted from incoming URLs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hour {
    Matutinum,
    Laudes,
    Prima,
    Tertia,
    Sexta,
    Nona,
    Vesperae,
    Completorium,
}

impl Hour {
    /// All hours in liturgical order, as rendered in the header.
    pub(crate) const ALL: [Hour; 8] = [
        Hour::Matutinum,
        Hour::Laudes,
        Hour::Prima,
        Hour::Tertia,
        Hour::Sexta,
        Hour::Nona,
        Hour::Vesperae,
        Hour::Completorium,
    ];

    /// Resolves an hour from a URL path segment, accepting both the canonical
    /// Latin name and common English/inflected aliases (case-insensitive).
    pub(crate) fn from_path(path: &str) -> Option<Hour> {
        Some(match path.to_ascii_lowercase().as_str() {
            "matins" | "matutinum" => Hour::Matutinum,
            "lauds" | "laudes" => Hour::Laudes,
            "prime" | "prima" => Hour::Prima,
            "terce" | "tertia" => Hour::Tertia,
            "sext" | "sexta" => Hour::Sexta,
            "none" | "nona" => Hour::Nona,
            "vespers" | "vespera" | "vesperae" => Hour::Vesperae,
            "compline" | "completorium" => Hour::Completorium,
            _ => return None,
        })
    }

    /// The canonical URL path segment for this hour.
    pub(crate) fn path(self) -> &'static str {
        match self {
            Hour::Matutinum => "matutinum",
            Hour::Laudes => "laudes",
            Hour::Prima => "prima",
            Hour::Tertia => "tertia",
            Hour::Sexta => "sexta",
            Hour::Nona => "nona",
            Hour::Vesperae => "vesperae",
            Hour::Completorium => "completorium",
        }
    }

    /// The human-readable label for this hour.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Hour::Matutinum => "Matutinum",
            Hour::Laudes => "Laudes",
            Hour::Prima => "Prima",
            Hour::Tertia => "Tertia",
            Hour::Sexta => "Sexta",
            Hour::Nona => "Nona",
            Hour::Vesperae => "Vesperae",
            Hour::Completorium => "Completorium",
        }
    }
}
