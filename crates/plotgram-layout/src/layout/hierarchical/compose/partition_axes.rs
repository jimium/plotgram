//! Orientation → partition cross/main axis mapping (partition-grid.md §5.1).
//!
//! Author `columns` / `rows` stay physical. Canonical TB maps them onto
//! the kernel's cross (x) / main (y) axes; OrientationOut maps bands back
//! so column intervals are physical left→right and row intervals top→bottom.

use plotgram_algo::orientation::Orientation;
use plotgram_model::partition::{PartitionCell, PartitionGrid};

use crate::layout::hierarchical::model::PartitionAxisKind;

/// Consumed partition axes for one orientation.
#[derive(Debug, Clone)]
pub struct ConsumedAxes {
    pub cross: PartitionAxisKind,
    pub main: PartitionAxisKind,
    /// Cross-axis ids in declaration order (layer-internal continuous blocks).
    pub cross_ids: Vec<String>,
    /// Main-axis ids in declaration order (rank intervals).
    pub main_ids: Vec<String>,
}

impl ConsumedAxes {
    pub fn from_grid(grid: &PartitionGrid, orientation: Orientation) -> Self {
        let columns: Vec<String> = grid.columns.iter().map(|a| a.id.clone()).collect();
        let rows: Vec<String> = grid.rows.iter().map(|a| a.id.clone()).collect();
        match orientation {
            Orientation::Tb | Orientation::Bt => Self {
                cross: PartitionAxisKind::Columns,
                main: PartitionAxisKind::Rows,
                cross_ids: columns,
                main_ids: rows,
            },
            Orientation::Lr | Orientation::Rl => Self {
                cross: PartitionAxisKind::Rows,
                main: PartitionAxisKind::Columns,
                cross_ids: rows,
                main_ids: columns,
            },
        }
    }

    pub fn is_consumed(&self) -> bool {
        !self.cross_ids.is_empty() || !self.main_ids.is_empty()
    }
}

pub fn cell_on(cell: &PartitionCell, kind: PartitionAxisKind) -> Option<&str> {
    match kind {
        PartitionAxisKind::Columns => cell.column.as_deref(),
        PartitionAxisKind::Rows => cell.row.as_deref(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plotgram_model::partition::PartitionAxis;

    fn grid() -> PartitionGrid {
        PartitionGrid {
            columns: vec![PartitionAxis::new("sales")],
            rows: vec![PartitionAxis::new("intake")],
        }
    }

    #[test]
    fn tb_maps_columns_to_cross_rows_to_main() {
        let a = ConsumedAxes::from_grid(&grid(), Orientation::Tb);
        assert_eq!(a.cross, PartitionAxisKind::Columns);
        assert_eq!(a.main, PartitionAxisKind::Rows);
        assert_eq!(a.cross_ids, ["sales"]);
        assert_eq!(a.main_ids, ["intake"]);
    }

    #[test]
    fn lr_maps_rows_to_cross_columns_to_main() {
        let a = ConsumedAxes::from_grid(&grid(), Orientation::Lr);
        assert_eq!(a.cross, PartitionAxisKind::Rows);
        assert_eq!(a.main, PartitionAxisKind::Columns);
        assert_eq!(a.cross_ids, ["intake"]);
        assert_eq!(a.main_ids, ["sales"]);
    }
}
