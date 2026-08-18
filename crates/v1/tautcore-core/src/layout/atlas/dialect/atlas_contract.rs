//! AtlasContract：各 Dialect 编译产物的统一枚举（Stage 6）。

use super::contract::HierarchicalContract;
use super::kind::DialectKind;
use super::scheme::SchemeId;

/// Dialect → 度量/落笔的契约。
#[derive(Debug, Clone)]
pub enum AtlasContract {
    Hierarchical(HierarchicalContract),
    /// Mindmap / Tree：节点由 TreeDialect 产出，边走 organic。
    Tree { scheme_id: SchemeId },
    /// Sequence：BuiltinEdges，布局即边几何。
    Sequence { scheme_id: SchemeId },
    /// Circular / ER / circular-state：圆排布 + curved 路由。
    Circular { scheme_id: SchemeId },
}

impl AtlasContract {
    pub fn kind(&self) -> DialectKind {
        match self {
            Self::Hierarchical(_) => DialectKind::Hierarchical,
            Self::Tree { .. } => DialectKind::Tree,
            Self::Sequence { .. } => DialectKind::Sequence,
            Self::Circular { .. } => DialectKind::Circular,
        }
    }

    pub fn scheme_id(&self) -> SchemeId {
        match self {
            Self::Hierarchical(c) => c.scheme_id,
            Self::Tree { scheme_id }
            | Self::Sequence { scheme_id }
            | Self::Circular { scheme_id } => *scheme_id,
        }
    }

    pub fn as_hierarchical(&self) -> Option<&HierarchicalContract> {
        match self {
            Self::Hierarchical(c) => Some(c),
            _ => None,
        }
    }
}
