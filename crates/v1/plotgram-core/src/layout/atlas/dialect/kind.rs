//! Dialect 种类（Stage 6 四内核）。

/// Atlas 四布局内核。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DialectKind {
    Hierarchical,
    Tree,
    Sequence,
    Circular,
}
