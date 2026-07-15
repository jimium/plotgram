//! 边路由共享工具

pub mod circular_support;
pub mod collinear_simplify;
pub mod edge_geometry;
pub mod label_avoidance;
pub mod label_candidate;
pub mod label_common;
pub mod label_placement;
pub mod obstacle_check;
pub mod parallel_edges;
pub mod routing_skeleton;
pub mod self_loop;
pub mod spatial_grid;

#[cfg(test)]
pub mod test_fixtures;
