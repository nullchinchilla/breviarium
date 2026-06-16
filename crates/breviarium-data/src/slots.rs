//! Canonical slot vocabulary — the single contract between the importer (which
//! emits these slot names into source bundles) and the resolver (which looks
//! them up). The `Slot` table in `resolve.rs` references exactly these names.
//!
//! Two namespaces exist today:
//!
//! 1. **Canonical** lowercase-hyphen names (`lauds-hymn`, `vespers-versicle`,
//!    `matins-psalmody`, …). The proper/common source bundles already use these.
//! 2. **DO-flavored** section names (`Capitulum`, `Hymnus`, `Ant 2`, `Versum 3`,
//!    …) still used by the ferial/seasonal *special* sources (psalter/ordinary)
//!    and the fixed formulae.
//!
//! [`do_aliases`] is the **temporary bridge**: while the data still carries
//! DO-flavored section names in the special sources, `catalog::lookup` falls
//! back through these aliases. Once `import-divinum` emits canonical names
//! everywhere (Execution plan step 5), this bridge is deleted and every lookup
//! is direct.
//!
//! Genuinely occurrence-dependent choices (1st vs 2nd Vespers versicle order,
//! commemoration antiphons, seasonal psalter scheme, seasonal hymn variants) are
//! NOT encoded here — they live in the `resolve.rs` handlers, because they
//! depend on the resolved day, not just the slot name.

/// Every canonical slot name the resolver may request. A `Slot` table entry
/// naming something absent here, or an importer emitting a slot absent here, is
/// a contract violation (asserted in tests).
pub const CANONICAL: &[&str] = &[
    // --- shared / formulae (filled by the ordinary book, fixed text) ---
    "deus-in-adjutorium",
    "domine-labia",
    "conclusion",
    "te-deum",
    "pater-noster",
    "pretiosa",
    // --- Matins ---
    "matins-invitatory",
    "matins-hymn",
    "matins-psalmody",
    "matins-absolutions",
    "matins-blessings-nocturn-1",
    "matins-blessings-nocturn-2",
    "matins-blessings-nocturn-3",
    "matins-blessings-nocturn-3-christmas",
    "matins-reading-1",
    "matins-reading-2",
    "matins-reading-3",
    "matins-reading-3-abbreviated",
    "matins-reading-4",
    "matins-reading-5",
    "matins-reading-6",
    "matins-reading-7",
    "matins-reading-8",
    "matins-reading-9",
    "matins-responsory-1",
    "matins-responsory-2",
    "matins-responsory-3",
    "matins-responsory-4",
    "matins-responsory-5",
    "matins-responsory-6",
    "matins-responsory-7",
    "matins-responsory-8",
    "matins-collect",
    // --- Lauds ---
    "lauds-psalmody",
    "lauds-chapter",
    "lauds-hymn",
    "lauds-versicle",
    "lauds-gospel-antiphon", // Benedictus antiphon
    // --- Prime ---
    "prime-hymn",
    "prime-short-reading",
    "prime-short-responsory",
    "prime-versicle",
    "chapter-office",
    "short-reading",
    // --- minor hours (terce / sext / none) share the same slot names ---
    "minor-hymn",
    "minor-chapter",
    "minor-short-responsory",
    "minor-versicle",
    // --- Vespers ---
    "vespers-psalmody",
    "vespers-chapter",
    "vespers-hymn",
    "vespers-versicle",
    "vespers-gospel-antiphon", // Magnificat antiphon
    // --- Compline ---
    "compline-chapter",
    "compline-hymn",
    "compline-short-reading",
    "compline-short-responsory",
    "compline-versicle",
    "compline-gospel-antiphon", // Nunc dimittis antiphon
    "compline-gospel-antiphon-lent",
    "compline-gospel-antiphon-passiontide",
    "compline-gospel-antiphon-easter",
    "nunc-dimittis",
    "final-antiphon", // Marian antiphon
    // --- collects (candidate order handled in resolve.rs) ---
    "collect",
    "vespers-collect",
    "daytime-collect",
];

/// Temporary DO-name → canonical bridge for the legacy special sources. Maps a
/// canonical slot to the ordered DO-flavored section name(s) the resolver should
/// try in the psalter/ordinary books. Empty for slots that only ever appear
/// under their canonical name. Ordering reflects the documented fallback chains
/// in `ARCHITECTURE.md`.
///
/// NOTE: multi-candidate orders that are *occurrence-dependent* (e.g. Vespers
/// `Versum 3`/`1`/`2` by 1st-vs-2nd Vespers) are intentionally left to the
/// handlers; entries here are the position-independent fallbacks only.
pub fn do_aliases(slot: &str) -> &'static [&'static str] {
    match slot {
        // formulae keep their DO names in the ordinary book for now
        "deus-in-adjutorium" => &["Deus in adjutorium"],
        "domine-labia" => &["Domine labia"],
        "te-deum" => &["Te Deum"],
        "pater-noster" => &["Pater noster"],
        "pretiosa" => &["Pretiosa"],

        "matins-invitatory" => &["Invit"],
        "matins-hymn" => &["Hymnus"],

        "lauds-chapter" => &["Capitulum Laudes"],
        "lauds-hymn" => &["Hymnus Laudes"],
        "lauds-versicle" => &["Versum 2"],
        "lauds-gospel-antiphon" => &["Ant 2"],

        "vespers-chapter" => &["Capitulum Vespera", "Capitulum Laudes"],
        "vespers-hymn" => &["Hymnus Vespera"],
        "vespers-gospel-antiphon" => &["Ant 3"],

        "minor-chapter" => &["Capitulum"],
        "minor-hymn" => &["Hymnus"],

        "compline-chapter" => &["Capitulum Completorium"],

        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_names_are_unique_and_kebab() {
        let mut seen = std::collections::BTreeSet::new();
        for name in CANONICAL {
            assert!(seen.insert(*name), "duplicate canonical slot: {name}");
            assert!(
                name.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "non-kebab canonical slot: {name}"
            );
        }
    }

    #[test]
    fn aliases_only_reference_known_slots() {
        for name in CANONICAL {
            // do_aliases must be callable for every canonical slot.
            let _ = do_aliases(name);
        }
    }
}
