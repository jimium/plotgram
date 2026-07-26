//! Scheme = Dialect + Profile 具名组合（23 §7–§8）。

use super::kind::DialectKind;
use super::profile::HierarchicalProfile;

/// 具名 Scheme 标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SchemeId(pub &'static str);

pub const HIERARCHICAL_FLOW_ORTHO: SchemeId = SchemeId("hierarchical-flow-ortho");
pub const HIERARCHICAL_ARCH_EQUAL_TRACK_ORTHO: SchemeId =
    SchemeId("hierarchical-arch-equal-track-ortho");
pub const HIERARCHICAL_STATE: SchemeId = SchemeId("hierarchical-state");
pub const CIRCULAR_STATE: SchemeId = SchemeId("circular-state");
pub const TREE_RADIAL_ORGANIC: SchemeId = SchemeId("tree-radial-organic");
pub const SEQUENCE_BUILTIN_EDGES: SchemeId = SchemeId("sequence-builtin-edges");
pub const CIRCULAR_DEFAULT: SchemeId = SchemeId("circular-default");

/// Dialect + Profile 绑定。
#[derive(Debug, Clone, Copy)]
pub struct Scheme {
    pub id: SchemeId,
    pub kind: DialectKind,
    /// Hierarchical 有意义；其它 Dialect 用 flowchart_default 占位。
    pub profile: HierarchicalProfile,
}

impl Scheme {
    pub fn hierarchical_flow_ortho() -> Self {
        Self {
            id: HIERARCHICAL_FLOW_ORTHO,
            kind: DialectKind::Hierarchical,
            profile: HierarchicalProfile::flowchart_default(),
        }
    }

    pub fn hierarchical_arch_equal_track_ortho() -> Self {
        Self {
            id: HIERARCHICAL_ARCH_EQUAL_TRACK_ORTHO,
            kind: DialectKind::Hierarchical,
            profile: HierarchicalProfile::architecture_default(),
        }
    }

    pub fn hierarchical_state() -> Self {
        Self {
            id: HIERARCHICAL_STATE,
            kind: DialectKind::Hierarchical,
            profile: HierarchicalProfile::state_default(),
        }
    }

    pub fn circular_state() -> Self {
        Self {
            id: CIRCULAR_STATE,
            kind: DialectKind::Circular,
            profile: HierarchicalProfile::flowchart_default(),
        }
    }

    pub fn tree_radial_organic() -> Self {
        Self {
            id: TREE_RADIAL_ORGANIC,
            kind: DialectKind::Tree,
            profile: HierarchicalProfile::flowchart_default(),
        }
    }

    pub fn sequence_builtin_edges() -> Self {
        Self {
            id: SEQUENCE_BUILTIN_EDGES,
            kind: DialectKind::Sequence,
            profile: HierarchicalProfile::flowchart_default(),
        }
    }

    pub fn circular_default() -> Self {
        Self {
            id: CIRCULAR_DEFAULT,
            kind: DialectKind::Circular,
            profile: HierarchicalProfile::flowchart_default(),
        }
    }

    /// 按 id 查表；未知 id 返回 None。
    pub fn lookup(id: &str) -> Option<Self> {
        match id {
            "hierarchical-flow-ortho" => Some(Self::hierarchical_flow_ortho()),
            "hierarchical-arch-equal-track-ortho" => {
                Some(Self::hierarchical_arch_equal_track_ortho())
            }
            "hierarchical-state" => Some(Self::hierarchical_state()),
            "circular-state" => Some(Self::circular_state()),
            "tree-radial-organic" => Some(Self::tree_radial_organic()),
            "sequence-builtin-edges" => Some(Self::sequence_builtin_edges()),
            "circular-default" => Some(Self::circular_default()),
            _ => None,
        }
    }

    /// 注册表中的全部 Scheme（确定性顺序）。
    pub fn registry() -> &'static [Scheme] {
        use std::sync::LazyLock;
        static REG: LazyLock<[Scheme; 7]> = LazyLock::new(|| {
            [
                Scheme::hierarchical_flow_ortho(),
                Scheme::hierarchical_arch_equal_track_ortho(),
                Scheme::hierarchical_state(),
                Scheme::circular_state(),
                Scheme::tree_radial_organic(),
                Scheme::sequence_builtin_edges(),
                Scheme::circular_default(),
            ]
        });
        &*REG
    }
}
