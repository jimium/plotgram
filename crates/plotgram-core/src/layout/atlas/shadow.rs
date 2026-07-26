//! Shadow 对拍器（Stage 0 交付 0.5，23 号文 §0.3 / §2）。
//!
//! 门禁关闭期的替代品：同一个图分别跑 legacy 与 atlas 管线，产出
//! [`ShadowReport`] 差异报告（节点坐标 / 组框 / lint 计数 / 画布尺寸 /
//! 相 I 可行性）。Stage 0 里 atlas 转发 legacy，报告应**逐项零差异**；
//! 后续 Stage 的预期退化以此报告为唯一口径记录。
//!
//! 确定性（AGENTS.md §2）：`LayoutResult.nodes` / `groups` 是 HashMap，
//! 所有 diff 遍历显式按 id 排序。
//!
//! 报告文本渲染：markdown 归本模块（[`ShadowReport::to_markdown`]），
//! HTML（SVG 并排）归 CLI `plotgram shadow` 子命令。

use crate::ast::Diagram;
use crate::error::DiagnosticError;
use crate::layout::atlas::pipeline::AtlasPipeline;
use crate::layout::pipeline::plan::LayoutPlan;
use crate::layout::quality::lint::{compute_lint_metrics, LintMetricsSummary};
use crate::layout::types::LayoutResult;

/// 浮点零差异容差（同源求解应 bit 级一致，容差仅吸收报告层的减法噪声）。
const EPS: f64 = 1e-9;

/// 单节点坐标差异（atlas − legacy）。
#[derive(Debug, Clone, PartialEq)]
pub struct NodeDiff {
    pub id: String,
    pub dx: f64,
    pub dy: f64,
}

/// 单组框差异（atlas − legacy，四边等价表达为位移 + 尺寸差）。
#[derive(Debug, Clone, PartialEq)]
pub struct GroupDiff {
    pub id: String,
    pub dx: f64,
    pub dy: f64,
    pub dw: f64,
    pub dh: f64,
}

/// 节点位移统计（|Δ| 的分位数，px）。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DisplacementStats {
    pub p50: f64,
    pub p95: f64,
    pub max: f64,
}

impl DisplacementStats {
    /// 从逐节点位移模长汇总（输入无序，内部排序）。
    fn from_magnitudes(mut mags: Vec<f64>) -> Self {
        if mags.is_empty() {
            return Self::default();
        }
        mags.sort_by(|a, b| a.total_cmp(b));
        let pick = |q: f64| {
            let idx = ((mags.len() as f64 - 1.0) * q).round() as usize;
            mags[idx.min(mags.len() - 1)]
        };
        Self {
            p50: pick(0.50),
            p95: pick(0.95),
            max: *mags.last().unwrap(),
        }
    }
}

/// Shadow 差异报告（23 号文 §0.3 的五项口径）。
#[derive(Debug, Clone, PartialEq)]
pub struct ShadowReport {
    /// 图名（由调用方给定，仅用于报告输出）。
    pub name: String,
    /// 逐节点坐标 diff（仅含非零项，按 id 升序）。
    pub node_diffs: Vec<NodeDiff>,
    /// 节点位移统计（对**全部**共有节点，含零位移）。
    pub node_stats: DisplacementStats,
    /// 仅一侧存在的节点 id（`(仅 legacy, 仅 atlas)`，各自按 id 升序）。
    pub node_only: (Vec<String>, Vec<String>),
    /// 逐组框 diff（仅含非零项，按 id 升序）。
    pub group_diffs: Vec<GroupDiff>,
    /// 仅一侧存在的组 id。
    pub group_only: (Vec<String>, Vec<String>),
    /// lint 计数（legacy / atlas 两份，判读交给读者与 `is_zero`）。
    pub lint_legacy: LintMetricsSummary,
    pub lint_atlas: LintMetricsSummary,
    /// 画布尺寸差（atlas − legacy）。
    pub canvas_dw: f64,
    pub canvas_dh: f64,
    /// 相 I 可行性（Plan 是否成功）。Stage 0 恒 `None`，Stage 1 接 Plan 后填充。
    pub plan_feasible: Option<bool>,
}

impl ShadowReport {
    /// 逐项零差异（Stage 0 的验收判据）。lint 整体比较（含计数字段与严重度轨），
    /// 后续 Stage 偏离时任一差异即报。
    pub fn is_zero(&self) -> bool {
        self.node_diffs.is_empty()
            && self.node_only.0.is_empty()
            && self.node_only.1.is_empty()
            && self.group_diffs.is_empty()
            && self.group_only.0.is_empty()
            && self.group_only.1.is_empty()
            && self.lint_legacy == self.lint_atlas
            && self.canvas_dw.abs() <= EPS
            && self.canvas_dh.abs() <= EPS
    }

    /// 单行摘要（stderr / CLI 汇总用）。
    pub fn summary_line(&self) -> String {
        if self.is_zero() {
            format!("[shadow] {}: 零差异", self.name)
        } else {
            format!(
                "[shadow] {}: 节点diff {} 个 (p95={:.2}px max={:.2}px) · 组diff {} 个 · lint {} · 画布 Δ({:.1},{:.1})",
                self.name,
                self.node_diffs.len(),
                self.node_stats.p95,
                self.node_stats.max,
                self.group_diffs.len(),
                if self.lint_legacy == self.lint_atlas { "持平" } else { "有变化" },
                self.canvas_dw,
                self.canvas_dh,
            )
        }
    }

    /// markdown 明细（CLI 报告与调试输出共用）。
    pub fn to_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("### {}\n\n", self.name));
        if self.is_zero() {
            out.push_str("逐项零差异 ✓\n");
            return out;
        }
        if !self.node_diffs.is_empty() {
            out.push_str(&format!(
                "- 节点 diff：{} 个（p50={:.2} p95={:.2} max={:.2} px）\n",
                self.node_diffs.len(),
                self.node_stats.p50,
                self.node_stats.p95,
                self.node_stats.max
            ));
            out.push_str("\n| 节点 | dx | dy |\n|---|---|---|\n");
            for d in &self.node_diffs {
                out.push_str(&format!("| {} | {:.2} | {:.2} |\n", d.id, d.dx, d.dy));
            }
            out.push('\n');
        }
        for (label, ids) in [("仅 legacy 节点", &self.node_only.0), ("仅 atlas 节点", &self.node_only.1)] {
            if !ids.is_empty() {
                out.push_str(&format!("- {}：{}\n", label, ids.join(", ")));
            }
        }
        if !self.group_diffs.is_empty() {
            out.push_str("\n| 组 | dx | dy | dw | dh |\n|---|---|---|---|---|\n");
            for g in &self.group_diffs {
                out.push_str(&format!(
                    "| {} | {:.2} | {:.2} | {:.2} | {:.2} |\n",
                    g.id, g.dx, g.dy, g.dw, g.dh
                ));
            }
            out.push('\n');
        }
        for (label, ids) in [("仅 legacy 组", &self.group_only.0), ("仅 atlas 组", &self.group_only.1)] {
            if !ids.is_empty() {
                out.push_str(&format!("- {}：{}\n", label, ids.join(", ")));
            }
        }
        if self.lint_legacy != self.lint_atlas {
            out.push_str(&format!(
                "- lint 变化：legacy {:?} → atlas {:?}\n",
                self.lint_legacy, self.lint_atlas
            ));
        }
        if self.canvas_dw.abs() > EPS || self.canvas_dh.abs() > EPS {
            out.push_str(&format!(
                "- 画布尺寸 Δ：({:.1}, {:.1})\n",
                self.canvas_dw, self.canvas_dh
            ));
        }
        out
    }
}

/// 对拍产物：两份布局 + 差异报告。
pub struct ShadowRun {
    pub legacy: LayoutResult,
    pub atlas: LayoutResult,
    pub report: ShadowReport,
}

/// 同图跑 legacy 与 atlas 两条管线并对拍。
///
/// 任一侧布局失败即整体失败（Stage 0 双侧同源，失败必同时发生；
/// 后续 Stage 若需「单侧失败也出报告」再扩展）。
pub fn run_shadow(
    name: &str,
    diagram: &Diagram,
    plan: &LayoutPlan,
) -> Result<ShadowRun, DiagnosticError> {
    let legacy =
        crate::layout::pipeline::runner::LayoutPipeline::new(diagram, plan).run()?;
    let atlas = AtlasPipeline::new(diagram, plan).run()?;
    let report = diff_layouts(name, diagram, &legacy, &atlas);
    Ok(ShadowRun {
        legacy,
        atlas,
        report,
    })
}

/// 纯函数对拍：不跑管线，只算差异（供测试与增量场景复用）。
pub fn diff_layouts(
    name: &str,
    diagram: &Diagram,
    legacy: &LayoutResult,
    atlas: &LayoutResult,
) -> ShadowReport {
    // 节点：按 id 排序遍历（HashMap 迭代序不稳定，★ 红线 §2）。
    let mut node_ids: Vec<&String> = legacy.nodes.keys().collect();
    node_ids.sort();
    let mut node_diffs = Vec::new();
    let mut mags = Vec::new();
    let mut only_legacy_nodes = Vec::new();
    for id in node_ids {
        let l = &legacy.nodes[id];
        match atlas.nodes.get(id) {
            Some(a) => {
                let (dx, dy) = (a.x - l.x, a.y - l.y);
                mags.push((dx * dx + dy * dy).sqrt());
                if dx.abs() > EPS || dy.abs() > EPS {
                    node_diffs.push(NodeDiff {
                        id: id.clone(),
                        dx,
                        dy,
                    });
                }
            }
            None => only_legacy_nodes.push(id.clone()),
        }
    }
    let mut only_atlas_nodes: Vec<String> = atlas
        .nodes
        .keys()
        .filter(|id| !legacy.nodes.contains_key(*id))
        .cloned()
        .collect();
    only_atlas_nodes.sort();

    // 组框（`keys_sorted` 已按 id 升序）。
    let mut group_diffs = Vec::new();
    let mut only_legacy_groups = Vec::new();
    for (id, l) in legacy.groups.iter_sorted() {
        match atlas.groups.get(id) {
            Some(a) => {
                let (dx, dy, dw, dh) =
                    (a.x - l.x, a.y - l.y, a.width - l.width, a.height - l.height);
                if dx.abs() > EPS || dy.abs() > EPS || dw.abs() > EPS || dh.abs() > EPS {
                    group_diffs.push(GroupDiff {
                        id: id.clone(),
                        dx,
                        dy,
                        dw,
                        dh,
                    });
                }
            }
            None => only_legacy_groups.push(id.clone()),
        }
    }
    let only_atlas_groups: Vec<String> = atlas
        .groups
        .keys_sorted()
        .into_iter()
        .filter(|id| !legacy.groups.contains_key(id))
        .collect();

    ShadowReport {
        name: name.to_string(),
        node_diffs,
        node_stats: DisplacementStats::from_magnitudes(mags),
        node_only: (only_legacy_nodes, only_atlas_nodes),
        group_diffs,
        group_only: (only_legacy_groups, only_atlas_groups),
        lint_legacy: compute_lint_metrics(diagram, legacy),
        lint_atlas: compute_lint_metrics(diagram, atlas),
        canvas_dw: atlas.total_width - legacy.total_width,
        canvas_dh: atlas.total_height - legacy.total_height,
        plan_feasible: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prepared_small() -> crate::ast::PreparedDiagram {
        let source = "diagram flowchart {\n    entity a \"A\"\n    entity b \"B\"\n    entity c \"C\"\n    a -> b\n    b -> c\n}";
        let output = crate::pipeline::parse_prepare_validate(
            source,
            &crate::prepare::StyleRequest::default(),
        );
        assert!(output.is_valid(), "{:?}", output.errors);
        output.diagram.unwrap()
    }

    /// Stage 0 验收判据：atlas 转发 legacy，对拍应逐项零差异。
    #[test]
    fn shadow_run_is_zero_diff_in_stage0() {
        let prepared = prepared_small();
        let run = run_shadow("small", prepared.inner(), prepared.layout_plan()).unwrap();
        assert!(run.report.is_zero(), "{}", run.report.to_markdown());
        assert_eq!(run.report.summary_line(), "[shadow] small: 零差异");
        assert_eq!(run.report.plan_feasible, None, "S0 相 I 可行性恒占位");
    }

    /// diff 纯函数：人工扰动一个节点坐标与画布尺寸，报告应捕捉到。
    #[test]
    fn diff_layouts_detects_perturbation() {
        let prepared = prepared_small();
        let legacy = crate::layout::pipeline::runner::LayoutPipeline::new(
            prepared.inner(),
            prepared.layout_plan(),
        )
        .run()
        .unwrap();
        let mut atlas = legacy.clone();
        let first_id = {
            let mut ids: Vec<&String> = atlas.nodes.keys().collect();
            ids.sort();
            ids[0].clone()
        };
        atlas.nodes.get_mut(&first_id).unwrap().x += 5.0;
        atlas.total_width += 5.0;

        let report = diff_layouts("perturbed", prepared.inner(), &legacy, &atlas);
        assert!(!report.is_zero());
        assert_eq!(report.node_diffs.len(), 1);
        assert_eq!(report.node_diffs[0].id, first_id);
        assert!((report.node_diffs[0].dx - 5.0).abs() < 1e-9);
        assert!((report.node_stats.max - 5.0).abs() < 1e-9);
        assert!((report.canvas_dw - 5.0).abs() < 1e-9);
        assert!(report.summary_line().contains("perturbed"));
    }
}
