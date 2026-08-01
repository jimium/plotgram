//! PartitionGrid: orthogonal partition of the drawing (swimlanes / matrix).
//!
//! Distinct from [`crate::graph::Group`] (nesting) and from channel "lane" tracks.
//! See ADR-008 and `docs/design/layout/shared/partition.md`.

use std::collections::BTreeSet;
use std::fmt;

use crate::graph::Graph;

/// One axis entry (a column or a row) in a [`PartitionGrid`].
///
/// Declaration order in the parent `Vec` is the geometric axis order (stable).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PartitionAxis {
    /// Axis id (`[a-z][a-z0-9_]*`). Shares the diagram id space with nodes/groups.
    pub id: String,
    /// Optional display label (e.g. swimlane title).
    #[serde(default)]
    pub label: Option<String>,
}

impl PartitionAxis {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: None,
        }
    }

    pub fn with_label(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: Some(label.into()),
        }
    }
}

/// Orthogonal partition grid on a diagram.
///
/// - Columns only (or rows only) → swimlanes relative to flow direction.
/// - Both → matrix (e.g. phase × role).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct PartitionGrid {
    /// Column axes; declaration order = left-to-right (under TB flow).
    #[serde(default)]
    pub columns: Vec<PartitionAxis>,
    /// Row axes; empty = column-only swimlanes. Declaration order = along the row axis.
    #[serde(default)]
    pub rows: Vec<PartitionAxis>,
}

impl PartitionGrid {
    pub fn is_empty(&self) -> bool {
        self.columns.is_empty() && self.rows.is_empty()
    }

    /// All axis ids in declaration order (columns first, then rows).
    pub fn axis_ids(&self) -> impl Iterator<Item = &str> {
        self.columns
            .iter()
            .chain(self.rows.iter())
            .map(|a| a.id.as_str())
    }

    pub fn has_column(&self, id: &str) -> bool {
        self.columns.iter().any(|a| a.id == id)
    }

    pub fn has_row(&self, id: &str) -> bool {
        self.rows.iter().any(|a| a.id == id)
    }
}

/// Cell assignment for a node: optional column and/or row axis ids.
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct PartitionCell {
    #[serde(default)]
    pub column: Option<String>,
    #[serde(default)]
    pub row: Option<String>,
}

impl PartitionCell {
    pub fn col(column: impl Into<String>) -> Self {
        Self {
            column: Some(column.into()),
            row: None,
        }
    }

    pub fn row(row: impl Into<String>) -> Self {
        Self {
            column: None,
            row: Some(row.into()),
        }
    }

    pub fn at(column: impl Into<String>, row: impl Into<String>) -> Self {
        Self {
            column: Some(column.into()),
            row: Some(row.into()),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.column.is_none() && self.row.is_none()
    }
}

/// Validation errors for partition grid ↔ cell consistency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartitionError {
    /// Node has a cell but the graph has no partition grid.
    CellWithoutGrid { node_id: String },
    /// `cell_col` / column id not in `grid.columns`.
    UnknownColumn { node_id: String, column: String },
    /// `cell_row` / row id not in `grid.rows`.
    UnknownRow { node_id: String, row: String },
    /// Duplicate axis id within the grid (column/row lists).
    DuplicateAxisId { id: String },
    /// Axis id collides with a node or group id.
    AxisIdConflict { id: String },
}

impl fmt::Display for PartitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CellWithoutGrid { node_id } => {
                write!(
                    f,
                    "node `{node_id}` has partition cell but diagram has no `partition` grid"
                )
            }
            Self::UnknownColumn { node_id, column } => {
                write!(
                    f,
                    "node `{node_id}`: unknown partition column `{column}`"
                )
            }
            Self::UnknownRow { node_id, row } => {
                write!(f, "node `{node_id}`: unknown partition row `{row}`")
            }
            Self::DuplicateAxisId { id } => {
                write!(f, "partition axis id `{id}` declared more than once")
            }
            Self::AxisIdConflict { id } => {
                write!(
                    f,
                    "partition axis id `{id}` conflicts with a node or group id"
                )
            }
        }
    }
}

impl std::error::Error for PartitionError {}

/// Validate grid shape and every node's `partition_cell` against the grid.
///
/// Nodes without a cell are allowed (unassigned free zone).
pub fn validate_graph_partition(graph: &Graph) -> Result<(), PartitionError> {
    if let Some(grid) = &graph.partition {
        validate_grid_axes(grid)?;
        validate_axis_id_space(graph, grid)?;
    }

    for node in graph.all_nodes() {
        if let Some(cell) = &node.partition_cell {
            if cell.is_empty() {
                continue;
            }
            let Some(grid) = &graph.partition else {
                return Err(PartitionError::CellWithoutGrid {
                    node_id: node.id.clone(),
                });
            };
            if let Some(col) = &cell.column {
                if !grid.has_column(col) {
                    return Err(PartitionError::UnknownColumn {
                        node_id: node.id.clone(),
                        column: col.clone(),
                    });
                }
            }
            if let Some(row) = &cell.row {
                if !grid.has_row(row) {
                    return Err(PartitionError::UnknownRow {
                        node_id: node.id.clone(),
                        row: row.clone(),
                    });
                }
            }
        }
    }
    Ok(())
}

fn validate_grid_axes(grid: &PartitionGrid) -> Result<(), PartitionError> {
    let mut seen = BTreeSet::new();
    for id in grid.axis_ids() {
        if !seen.insert(id.to_string()) {
            return Err(PartitionError::DuplicateAxisId {
                id: id.to_string(),
            });
        }
    }
    Ok(())
}

fn validate_axis_id_space(graph: &Graph, grid: &PartitionGrid) -> Result<(), PartitionError> {
    let mut occupied = BTreeSet::new();
    for n in graph.all_nodes() {
        occupied.insert(n.id.as_str());
    }
    collect_group_ids(graph, &mut occupied);
    for id in grid.axis_ids() {
        if occupied.contains(id) {
            return Err(PartitionError::AxisIdConflict {
                id: id.to_string(),
            });
        }
    }
    Ok(())
}

fn collect_group_ids<'a>(graph: &'a Graph, out: &mut BTreeSet<&'a str>) {
    fn walk<'a>(groups: &'a [crate::graph::Group], out: &mut BTreeSet<&'a str>) {
        for g in groups {
            out.insert(g.id.as_str());
            walk(&g.groups, out);
        }
    }
    walk(&graph.groups, out);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attr::AttrMap;
    use crate::graph::{Graph, Node, NodeRole};

    fn entity(id: &str) -> Node {
        Node {
            id: id.to_string(),
            label: Some(id.to_string()),
            shape: None,
            role: NodeRole::Entity,
            host_group: None,
            anchor: None,
            partition_cell: None,
            attrs: AttrMap::new(),
        }
    }

    #[test]
    fn validate_accepts_column_only_swimlane() {
        let mut g = Graph::new();
        g.partition = Some(PartitionGrid {
            columns: vec![
                PartitionAxis::with_label("customer", "客户"),
                PartitionAxis::with_label("sales", "销售"),
            ],
            rows: vec![],
        });
        let mut n = entity("place_order");
        n.partition_cell = Some(PartitionCell::col("customer"));
        g.nodes.push(n);
        g.nodes.push(entity("orphan")); // no cell — allowed
        validate_graph_partition(&g).unwrap();
    }

    #[test]
    fn validate_rejects_cell_without_grid() {
        let mut g = Graph::new();
        let mut n = entity("a");
        n.partition_cell = Some(PartitionCell::col("x"));
        g.nodes.push(n);
        assert!(matches!(
            validate_graph_partition(&g),
            Err(PartitionError::CellWithoutGrid { .. })
        ));
    }

    #[test]
    fn validate_rejects_unknown_column() {
        let mut g = Graph::new();
        g.partition = Some(PartitionGrid {
            columns: vec![PartitionAxis::new("sales")],
            rows: vec![],
        });
        let mut n = entity("a");
        n.partition_cell = Some(PartitionCell::col("missing"));
        g.nodes.push(n);
        assert!(matches!(
            validate_graph_partition(&g),
            Err(PartitionError::UnknownColumn { .. })
        ));
    }

    #[test]
    fn validate_rejects_axis_id_conflict_with_node() {
        let mut g = Graph::new();
        g.nodes.push(entity("sales"));
        g.partition = Some(PartitionGrid {
            columns: vec![PartitionAxis::new("sales")],
            rows: vec![],
        });
        assert!(matches!(
            validate_graph_partition(&g),
            Err(PartitionError::AxisIdConflict { .. })
        ));
    }

    #[test]
    fn validate_rejects_duplicate_axis_id() {
        let grid = PartitionGrid {
            columns: vec![PartitionAxis::new("a"), PartitionAxis::new("a")],
            rows: vec![],
        };
        assert!(matches!(
            validate_grid_axes(&grid),
            Err(PartitionError::DuplicateAxisId { .. })
        ));
    }

    #[test]
    fn matrix_cell_requires_both_axes_present() {
        let mut g = Graph::new();
        g.partition = Some(PartitionGrid {
            columns: vec![PartitionAxis::new("sales")],
            rows: vec![PartitionAxis::new("intake")],
        });
        let mut n = entity("a");
        n.partition_cell = Some(PartitionCell::at("sales", "intake"));
        g.nodes.push(n);
        validate_graph_partition(&g).unwrap();
    }
}
