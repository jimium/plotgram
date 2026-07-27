//! 离线校验：给定 `Diagram` + `LayoutResult`，反向构造 `RouteProblem` 并跑 H0–H6。
//!
//! **不接线生产**。供 Phase 1 确定性单测与硬约束违反清单产出。

use super::auditor::{RouteHardAuditReport, audit_problem_against_layout};
use super::capacity::{
    all_port_candidates, compile_min_separation_constraints, compile_port_capacity_constraints,
    port_to_id,
};
use super::graph::ResourceGraph;
use super::model::{
    ConstraintSource, ConstraintSourceKind, EdgeVariable, RouteHardConstraint, RouteObjective,
    RouteObjectiveKind, RouteProblem, RouteSolverConfig,
};
use crate::ast::Diagram;
use crate::layout::group::GroupRoutingContext;
use crate::layout::types::{LayoutResult, Port};
use std::collections::BTreeMap;

/// 离线汇总：多样本违规合并。
#[derive(Debug, Clone, Default)]
pub struct OfflineViolationSummary {
    pub reports: Vec<RouteHardAuditReport>,
}

impl OfflineViolationSummary {
    pub fn total_violations(&self) -> usize {
        self.reports.iter().map(|r| r.violations.len()).sum()
    }

    pub fn kind_totals(&self) -> BTreeMap<super::feasibility::HardConstraintKind, usize> {
        let mut m = BTreeMap::new();
        for r in &self.reports {
            for (k, c) in r.by_kind_counts() {
                *m.entry(k).or_insert(0) += c;
            }
        }
        m
    }

    /// 渲染为 Markdown 报告正文。
    pub fn to_markdown(&self, title: &str) -> String {
        use std::fmt::Write;
        let mut s = String::new();
        let _ = writeln!(s, "# {title}\n");
        let _ = writeln!(
            s,
            "> Phase 3 离线硬约束校验（H0–H5）。H6 由签名确定性单测覆盖。\n"
        );
        let _ = writeln!(s, "## 汇总\n");
        let _ = writeln!(s, "| 样本数 | 检查边数 | 违规总数 |");
        let _ = writeln!(s, "|---:|---:|---:|");
        let edges: usize = self.reports.iter().map(|r| r.edges_checked).sum();
        let _ = writeln!(
            s,
            "| {} | {} | {} |",
            self.reports.len(),
            edges,
            self.total_violations()
        );
        let _ = writeln!(s, "\n## 按硬约束种类\n");
        let _ = writeln!(s, "| 种类 | 次数 |");
        let _ = writeln!(s, "|---|---:|");
        for (k, c) in self.kind_totals() {
            let _ = writeln!(s, "| {k:?} | {c} |");
        }
        let _ = writeln!(s, "\n## 逐样本\n");
        for r in &self.reports {
            let _ = writeln!(
                s,
                "### `{}` — signature=`{:016x}` — checked={} violations={}\n",
                r.sample,
                r.problem_signature,
                r.edges_checked,
                r.violations.len()
            );
            if r.violations.is_empty() {
                let _ = writeln!(s, "_无 H0–H3 违规_\n");
                continue;
            }
            let _ = writeln!(s, "| edge | from → to | kind | detail |");
            let _ = writeln!(s, "|---:|---|---|---|");
            for v in &r.violations {
                let _ = writeln!(
                    s,
                    "| {} | {} → {} | {:?} | {} |",
                    v.edge_index, v.from, v.to, v.kind, v.detail
                );
            }
            let _ = writeln!(s);
        }
        s
    }
}

/// 从生产布局反向构造 `RouteProblem`（Phase 3：含端口候选域 + H4/H5）。
pub fn build_route_problem_from_layout(diagram: &Diagram, layout: &LayoutResult) -> RouteProblem {
    let mut nodes_bt: BTreeMap<String, crate::layout::types::NodeLayout> = BTreeMap::new();
    for (k, v) in &layout.nodes {
        nodes_bt.insert(k.clone(), v.clone());
    }
    let mut graph = ResourceGraph::new();
    graph.add_port_anchors_from_nodes(&nodes_bt);

    let mut edges = Vec::with_capacity(diagram.relations.len());
    let mut hard = Vec::new();
    let mut edge_ports: Vec<(usize, &str, Port, &str, Port)> = Vec::new();
    let mut sep_pairs: Vec<(usize, usize)> = Vec::new();

    for (i, rel) in diagram.relations.iter().enumerate() {
        let (from_port, to_port) = layout
            .edges
            .get(i)
            .map(|e| (e.from_port, e.to_port))
            .unwrap_or((Port::Bottom, Port::Top));
        // 决策域：四向候选（已选端口置前，保证签名稳定且含当前解）
        let mut from_cands = all_port_candidates();
        let mut to_cands = all_port_candidates();
        let fp = port_to_id(from_port);
        let tp = port_to_id(to_port);
        from_cands.sort_by_key(|&p| if p == fp { 0 } else { 1 + p });
        to_cands.sort_by_key(|&p| if p == tp { 0 } else { 1 + p });
        edges.push(EdgeVariable {
            edge: i,
            from_node: rel.from.as_str().to_string(),
            to_node: rel.to.as_str().to_string(),
            from_port_candidates: from_cands,
            to_port_candidates: to_cands,
        });
        edge_ports.push((
            i,
            rel.from.as_str(),
            from_port,
            rel.to.as_str(),
            to_port,
        ));
        hard.push(RouteHardConstraint::EndpointOnBoundary {
            edge: i,
            node: rel.from.as_str().to_string(),
            port: fp,
            source: ConstraintSource {
                kind: ConstraintSourceKind::EndpointGeometry,
                entities: vec![rel.from.as_str().to_string()],
                note: "H0 from",
            },
        });
        hard.push(RouteHardConstraint::EndpointOnBoundary {
            edge: i,
            node: rel.to.as_str().to_string(),
            port: tp,
            source: ConstraintSource {
                kind: ConstraintSourceKind::EndpointGeometry,
                entities: vec![rel.to.as_str().to_string()],
                note: "H0 to",
            },
        });
        for nid in layout.nodes.keys() {
            if nid == rel.from.as_str() || nid == rel.to.as_str() {
                continue;
            }
            hard.push(RouteHardConstraint::ObstacleClearance {
                edge: i,
                obstacle: nid.clone(),
                min: 0.0,
                source: ConstraintSource {
                    kind: ConstraintSourceKind::NodeObstacle,
                    entities: vec![nid.clone()],
                    note: "H1 third-party node",
                },
            });
        }
    }

    // 同无序端点对 → H5 候选对
    let mut by_pair: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
    for (i, rel) in diagram.relations.iter().enumerate() {
        let a = rel.from.as_str();
        let b = rel.to.as_str();
        let key = if a <= b {
            (a.to_string(), b.to_string())
        } else {
            (b.to_string(), a.to_string())
        };
        by_pair.entry(key).or_default().push(i);
    }
    for members in by_pair.values() {
        if members.len() < 2 {
            continue;
        }
        for i in 0..members.len() {
            for j in (i + 1)..members.len() {
                sep_pairs.push((members[i], members[j]));
            }
        }
    }
    // 默认间距 8（flowchart）；审计用统一阈值
    hard.extend(compile_min_separation_constraints(&sep_pairs, 8.0));
    hard.extend(compile_port_capacity_constraints(&edge_ports));

    let objectives = vec![
        RouteObjective {
            kind: RouteObjectiveKind::Crossings,
            weight: 1.0,
            note: "Q2",
        },
        RouteObjective {
            kind: RouteObjectiveKind::Bends,
            weight: 1.0,
            note: "Q3",
        },
        RouteObjective {
            kind: RouteObjectiveKind::Length,
            weight: 1.0,
            note: "Q4",
        },
    ];

    RouteProblem {
        graph,
        edges,
        hard,
        objectives,
        config: RouteSolverConfig::default(),
    }
}

/// `RouteProblem` 确定性签名（对外别名）。
pub fn problem_signature(problem: &RouteProblem) -> u64 {
    problem.signature()
}

/// 对单个已布局结果跑离线硬审计。
pub fn audit_layout_result(
    sample_name: &str,
    diagram: &Diagram,
    layout: &LayoutResult,
) -> RouteHardAuditReport {
    let problem = build_route_problem_from_layout(diagram, layout);
    let group_ctx = GroupRoutingContext::from_layout(diagram, layout, "spline");
    audit_problem_against_layout(sample_name, &problem, diagram, layout, &group_ctx)
}

/// 批量审计（声明序样本名）。
pub fn audit_many(
    samples: &[(String, Diagram, LayoutResult)],
) -> OfflineViolationSummary {
    let mut reports = Vec::with_capacity(samples.len());
    for (name, diagram, layout) in samples {
        reports.push(audit_layout_result(name, diagram, layout));
    }
    OfflineViolationSummary { reports }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::pipeline::entry::compute_layout;

    fn tiny_flowchart() -> Diagram {
        let src = r#"
diagram flowchart {
    entity[process] a "Start"
    entity[process] b "End"
    a -> b
}
"#;
        crate::dsl::parser::parse(src).expect("parse")
    }

    #[test]
    fn route_problem_signature_is_deterministic() {
        let diagram = tiny_flowchart();
        let layout = compute_layout(&diagram).expect("layout");
        let p1 = build_route_problem_from_layout(&diagram, &layout);
        let p2 = build_route_problem_from_layout(&diagram, &layout);
        assert_eq!(p1.signature(), p2.signature());
        assert_eq!(problem_signature(&p1), problem_signature(&p2));
    }

    #[test]
    fn offline_audit_runs_on_tiny_flowchart() {
        let diagram = tiny_flowchart();
        let layout = compute_layout(&diagram).expect("layout");
        let report = audit_layout_result("tiny", &diagram, &layout);
        assert_eq!(report.edges_checked, 1);
        assert_eq!(report.sample, "tiny");
        let _ = report.by_kind_counts();
    }

    /// 跑 product-regression 全集并写入硬约束违反清单。
    /// 日常 `cargo test` 跳过；显式：
    /// `cargo test -p plotgram-core --lib write_phase3_hard_constraint_report -- --ignored --nocapture`
    #[test]
    #[ignore = "writes docs/优化重构/26-Phase3-硬约束违反清单-2026-07.md"]
    fn write_phase3_hard_constraint_report() {
        use std::path::PathBuf;
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root = manifest.join("../..");
        let set_path = root.join("benchmarks/sets/product-regression-set.txt");
        let set = std::fs::read_to_string(&set_path).expect("read product-regression-set");
        let mut reports = Vec::new();
        for line in set.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let path = root.join(line);
            let src = match std::fs::read_to_string(&path) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("skip {line}: {e}");
                    continue;
                }
            };
            let diagram = match crate::dsl::parser::parse(&src) {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("parse fail {line}: {e:?}");
                    continue;
                }
            };
            let layout = match compute_layout(&diagram) {
                Ok(l) => l,
                Err(e) => {
                    eprintln!("layout fail {line}: {e:?}");
                    continue;
                }
            };
            let report = audit_layout_result(line, &diagram, &layout);
            eprintln!(
                "audited {line}: edges={} violations={}",
                report.edges_checked,
                report.violations.len()
            );
            reports.push(report);
        }
        let summary = OfflineViolationSummary { reports };
        let md = summary.to_markdown(
            "Phase 3 硬约束违反清单（端口/lane 联合求解后，product-regression）",
        );
        let out = root.join("docs/优化重构/26-Phase3-硬约束违反清单-2026-07.md");
        std::fs::write(&out, &md).expect("write report");
        eprintln!("wrote {} ({} bytes)", out.display(), md.len());
        assert!(
            !summary.reports.is_empty(),
            "expected at least one audited sample"
        );
    }
}
