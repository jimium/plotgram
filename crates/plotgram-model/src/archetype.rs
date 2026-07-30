//! Archetype definitions (dsl-spec §5.6 / archetype-spec).
//!
//! An archetype is a **named combination pack** over the three visual axes
//! (`shape × variant × icon`). Authors write one atom; the expander fills
//! missing axes before profile defaults apply.
//!
//! Compile-time static table — no CSV, no build.rs, no runtime IO.
//! WASM-safe. Deterministic order = declaration order (AGENTS.md §2).

/// A named combination pack expanding into shape / variant / icon defaults.
///
/// Semantics: **fill-only** — never overrides an axis the author already set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArchetypeDef {
    /// Normalized id (lowercase, `-` → `_`).
    pub id: &'static str,
    /// Default shape (closed set; `None` = not provided by this pack).
    pub shape: Option<&'static str>,
    /// Default variant (closed set; `None` = not provided).
    pub variant: Option<&'static str>,
    /// Default icon id or literal `none` (`None` = not provided).
    pub icon: Option<&'static str>,
}

/// Built-in archetype catalog (archetype-spec §5.4).
///
/// Order = canonical declaration order; lookup is linear (≤ 32 entries).
pub static ARCHETYPES: &[ArchetypeDef] = &[
    ArchetypeDef { id: "database",  shape: Some("cylinder"),      variant: Some("info"),    icon: None },
    ArchetypeDef { id: "cache",     shape: Some("cylinder"),      variant: Some("info"),    icon: Some("cache") },
    ArchetypeDef { id: "queue",     shape: Some("stadium"),       variant: Some("info"),    icon: Some("queue") },
    ArchetypeDef { id: "storage",   shape: Some("cylinder"),      variant: Some("info"),    icon: Some("storage") },
    ArchetypeDef { id: "gateway",   shape: Some("diamond"),       variant: Some("default"), icon: None },
    ArchetypeDef { id: "external",  shape: Some("rounded_rect"),  variant: Some("muted"),   icon: Some("external") },
    ArchetypeDef { id: "service",   shape: Some("rounded_rect"),  variant: Some("default"), icon: Some("service") },
    ArchetypeDef { id: "client",    shape: Some("rounded_rect"),  variant: Some("default"), icon: Some("client") },
    ArchetypeDef { id: "decision",  shape: Some("diamond"),       variant: Some("default"), icon: None },
    ArchetypeDef { id: "start",     shape: Some("circle"),        variant: Some("primary"), icon: None },
    ArchetypeDef { id: "end",       shape: Some("circle"),        variant: Some("muted"),   icon: None },
    ArchetypeDef { id: "actor",     shape: Some("person"),        variant: Some("default"), icon: None },
    ArchetypeDef { id: "root",      shape: Some("rounded_rect"),  variant: Some("primary"), icon: None },
];

/// Normalize an archetype id: lowercase, trim, `-` → `_`.
pub fn normalize_id(raw: &str) -> String {
    raw.trim().to_ascii_lowercase().replace('-', "_")
}

/// Lookup by raw id (normalizes internally).
///
/// Unknown archetype → `None` (no error; archetype-spec §4: diagnose-only).
pub fn archetype_by_id(raw: &str) -> Option<&'static ArchetypeDef> {
    let normalized = normalize_id(raw);
    ARCHETYPES.iter().find(|a| a.id == normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_basic() {
        let def = archetype_by_id("database").unwrap();
        assert_eq!(def.shape, Some("cylinder"));
        assert_eq!(def.variant, Some("info"));
        assert_eq!(def.icon, None);
    }

    #[test]
    fn lookup_normalizes_case_and_hyphen() {
        // Case normalization
        assert_eq!(archetype_by_id("GATEWAY").unwrap().id, "gateway");
        assert_eq!(archetype_by_id("DataBase").unwrap().id, "database");
        // Hyphen → underscore (e.g. if we had a hyphenated id)
        assert_eq!(normalize_id("My-Arch"), "my_arch");
    }

    #[test]
    fn lookup_unknown_returns_none() {
        assert!(archetype_by_id("nonexistent").is_none());
        assert!(archetype_by_id("").is_none());
    }

    #[test]
    fn catalog_ids_unique_and_sorted() {
        let mut seen = std::collections::BTreeSet::new();
        for def in ARCHETYPES {
            assert!(seen.insert(def.id), "duplicate archetype id: {}", def.id);
        }
    }

    #[test]
    fn all_entries_have_shape() {
        // Every built-in archetype should provide at least a shape.
        for def in ARCHETYPES {
            assert!(def.shape.is_some(), "archetype `{}` missing shape", def.id);
        }
    }
}
