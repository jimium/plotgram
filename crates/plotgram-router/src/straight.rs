//! Independent straight [`EdgeRouter`](plotgram_engine_api::EdgeRouter).
//!
//! Connects each edge's source and target port anchors with a two-point
//! polyline. No obstacle avoidance, no bends, no track separation.
//!
//! Honesty (R5): group scenes are rejected — a straight segment cannot
//! express gate-mediated boundary crossings.

use plotgram_engine_api::{EdgeRouter, LayoutError, RouteScene};
use plotgram_model::port::PortRef;
use plotgram_model::result::{EdgePath, EdgePlacement};

/// Straight edge router registry name: `"straight"`.
#[derive(Debug, Default, Clone, Copy)]
pub struct StraightEdgeRouter;

impl EdgeRouter for StraightEdgeRouter {
    fn name(&self) -> &'static str {
        "straight"
    }

    fn route(&self, scene: &RouteScene) -> Result<Vec<EdgePlacement>, LayoutError> {
        if let Err(reason) = scene.validate() {
            return Err(LayoutError::UnsupportedRouteScene { reason });
        }
        if !scene.group_boundaries.is_empty() || !scene.boundary_permissions.is_empty() {
            return Err(LayoutError::UnsupportedRouteScene {
                reason: "straight router does not support group boundaries \
                         (cannot express gate crossings)"
                    .to_string(),
            });
        }

        let mut out = Vec::with_capacity(scene.edge_order.len());
        for edge_id in &scene.edge_order {
            let pair = scene.terminals.get(edge_id).ok_or_else(|| {
                LayoutError::message(format!(
                    "router: edge `{edge_id}` in edge_order but missing from terminals"
                ))
            })?;
            out.push(EdgePlacement {
                id: edge_id.clone(),
                source: pair.source.node_id.clone(),
                target: pair.target.node_id.clone(),
                path: EdgePath::polyline(vec![pair.source.point, pair.target.point]),
                from_port: Some(PortRef {
                    side: pair.source.side,
                    along: plotgram_model::port::AlongSpec::Ordered { order: 0, count: 1 },
                }),
                to_port: Some(PortRef {
                    side: pair.target.side,
                    along: plotgram_model::port::AlongSpec::Ordered { order: 0, count: 1 },
                }),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use plotgram_engine_api::{
        GroupBoundary, Obstacle, OrthogonalRouteParams, PortAnchor, TerminalPair,
    };
    use plotgram_model::geometry::{Point, Rect};
    use plotgram_model::port::Side;
    use std::collections::BTreeMap;

    fn obstacle(id: &str, x: f64, y: f64, w: f64, h: f64) -> Obstacle {
        Obstacle {
            id: id.to_string(),
            rect: Rect::new(x, y, w, h),
        }
    }

    fn anchor(x: f64, y: f64, side: Side, node: &str) -> PortAnchor {
        PortAnchor {
            point: Point { x, y },
            side,
            node_id: node.to_string(),
        }
    }

    fn scene(obstacles: Vec<Obstacle>, source: PortAnchor, target: PortAnchor) -> RouteScene {
        let mut terminals = BTreeMap::new();
        terminals.insert("e0".to_string(), TerminalPair { source, target });
        RouteScene {
            obstacles,
            terminals,
            edge_order: vec!["e0".to_string()],
            group_boundaries: vec![],
            boundary_permissions: BTreeMap::new(),
            params: OrthogonalRouteParams::default(),
        }
    }

    fn two_boxes() -> RouteScene {
        scene(
            vec![
                obstacle("a", 0.0, 0.0, 80.0, 40.0),
                obstacle("b", 200.0, 100.0, 80.0, 40.0),
            ],
            anchor(80.0, 20.0, Side::East, "a"),
            anchor(200.0, 120.0, Side::West, "b"),
        )
    }

    #[test]
    fn connects_terminals_with_two_points() {
        let cases = [
            (
                "diagonal",
                two_boxes(),
                Point { x: 80.0, y: 20.0 },
                Point { x: 200.0, y: 120.0 },
            ),
            (
                "horizontal",
                scene(
                    vec![
                        obstacle("a", 0.0, 0.0, 80.0, 40.0),
                        obstacle("b", 200.0, 0.0, 80.0, 40.0),
                    ],
                    anchor(80.0, 20.0, Side::East, "a"),
                    anchor(200.0, 20.0, Side::West, "b"),
                ),
                Point { x: 80.0, y: 20.0 },
                Point { x: 200.0, y: 20.0 },
            ),
        ];
        for (name, sc, from, to) in cases {
            let p = StraightEdgeRouter.route(&sc).unwrap();
            assert_eq!(p.len(), 1, "{name}");
            assert_eq!(p[0].path.polyline_points().unwrap(), &vec![from, to][..], "{name}");
            assert_eq!(p[0].source, "a");
            assert_eq!(p[0].target, "b");
            assert_eq!(p[0].from_port.unwrap().side, Side::East);
            assert_eq!(p[0].to_port.unwrap().side, Side::West);
        }
    }

    #[test]
    fn ignores_obstacles_between_terminals() {
        // Straight may cross obstacles — that is the style, not a failure.
        let sc = scene(
            vec![
                obstacle("a", 0.0, 80.0, 80.0, 40.0),
                obstacle("b", 300.0, 80.0, 80.0, 40.0),
                obstacle("blocker", 150.0, 60.0, 80.0, 80.0),
            ],
            anchor(80.0, 100.0, Side::East, "a"),
            anchor(300.0, 100.0, Side::West, "b"),
        );
        let p = StraightEdgeRouter.route(&sc).unwrap();
        assert_eq!(
            p[0].path.polyline_points().unwrap(),
            &[Point { x: 80.0, y: 100.0 }, Point { x: 300.0, y: 100.0 }]
        );
    }

    #[test]
    fn group_scene_is_unsupported() {
        let mut sc = two_boxes();
        sc.group_boundaries.push(GroupBoundary {
            group_id: "g".to_string(),
            rect: Rect::new(100.0, 0.0, 40.0, 40.0),
        });
        match StraightEdgeRouter.route(&sc) {
            Err(LayoutError::UnsupportedRouteScene { reason }) => {
                assert!(reason.contains("group"), "{reason}");
            }
            other => panic!("expected UnsupportedRouteScene, got {other:?}"),
        }
    }

    #[test]
    fn multi_edge_preserves_order() {
        let mut terminals = BTreeMap::new();
        terminals.insert(
            "e1".to_string(),
            TerminalPair {
                source: anchor(80.0, 10.0, Side::East, "a"),
                target: anchor(200.0, 10.0, Side::West, "b"),
            },
        );
        terminals.insert(
            "e0".to_string(),
            TerminalPair {
                source: anchor(80.0, 30.0, Side::East, "a"),
                target: anchor(200.0, 30.0, Side::West, "b"),
            },
        );
        let sc = RouteScene {
            obstacles: vec![
                obstacle("a", 0.0, 0.0, 80.0, 40.0),
                obstacle("b", 200.0, 0.0, 80.0, 40.0),
            ],
            terminals,
            edge_order: vec!["e0".to_string(), "e1".to_string()],
            group_boundaries: vec![],
            boundary_permissions: BTreeMap::new(),
            params: OrthogonalRouteParams::default(),
        };
        let p = StraightEdgeRouter.route(&sc).unwrap();
        assert_eq!(p.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["e0", "e1"]);
        assert_eq!(p[0].path.polyline_points().unwrap()[0].y, 30.0);
        assert_eq!(p[1].path.polyline_points().unwrap()[0].y, 10.0);
    }

    #[test]
    fn double_run_is_bit_identical() {
        let sc = two_boxes();
        let a = StraightEdgeRouter.route(&sc).unwrap();
        let b = StraightEdgeRouter.route(&sc).unwrap();
        assert_eq!(a.len(), b.len());
        for (pa, pb) in a.iter().zip(b.iter()) {
            assert_eq!(pa.id, pb.id);
            assert_eq!(pa.path, pb.path);
        }
    }
}
