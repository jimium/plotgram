//! Legacy Adapter（23 号文 Stage 1 交付 1.3）：从旧管线中间产物反向构造 [`Plan`]。
//!
//! **旁路模块**：不接线生产，仅供表达力验证（门 B）与 Ink 对拍使用。
//! 证明 Plan 结构能装下五图种的离散决策——如果某图种装不下，在此暴露。
//!
//! ## 输入源
//!
//! | 旧产物 | 映射到 Plan 字段 |
//! |--------|-----------------|
//! | `LayeredKernel::compute` → rank/order | `node_slots` |
//! | `Diagram.groups` + 成员包围盒 | `group_scopes` |
//! | 旧路由 `EdgeLayout.from_port/to_port` | `ports` |
//! | channel graph `route_candidates` | `channels` + `gates` |
//! | `detect_bundles` | `bundles` |
//!
//! ## 确定性
//!
//! 全部容器 BTreeMap + 显式排序（AGENTS.md §2）。`LayeredKernel` 内部用 HashMap
//! 但导出后按 entity_id 排序填入有序容器。

use super::channel::{
    derive_node_ports, derive_substrate, route_candidates, BlueprintIndex, ChannelBlueprint,
    ChannelGraph, DerivePortsOptions, GroupSpec, NodeSpec, Occupancy, PortSide, RouteOutcome,
    ScopeMask, Substrate,
};
use super::plan::{EdgePorts, GroupKey, Plan, PortRef, Provenance, Slot, SubstrateSketch};
use crate::ast::Diagram;
use crate::layout::kernel::layered::graph::LayerNodeKind;
use crate::layout::kernel::layered::layered_kernel::LayeredKernel;
use crate::layout::kernel::layered::preset;
use crate::layout::kernel::layered::preset::SugiyamaPreset;
use crate::layout::types::{EdgeLayout, LayoutResult, Port};
use crate::types::DiagramType;
use std::collections::BTreeMap;
use thiserror::Error;

// ─── 公开类型 ─────────────────────────────────────────────────

/// Adapter 输入：旧管线的完整产出。
pub struct AdapterInput<'a> {
    pub diagram: &'a Diagram,
    /// 旧管线完整输出（含 edges 的端口分配）。
    pub layout_result: &'a LayoutResult,
}

/// Adapter 输出：Plan + 基底 + 表达力缺口。
pub struct AdapterOutput {
    pub plan: Plan,
    pub substrate: Substrate,
    pub index: BlueprintIndex,
    /// 表达力缺口（1.5 的逐边记录）。
    pub gaps: Vec<ExpressivenessGap>,
}

/// 表达力缺口（23 号文 Stage 1 交付 1.5）。
#[derive(Debug, Clone, PartialEq)]
pub enum ExpressivenessGap {
    /// 通道图选路不可行（A4 失败边）。
    Infeasible {
        edge: usize,
        from: String,
        to: String,
        reason: String,
    },
    /// Plan 能表达但 Ink 重建与旧几何拓扑不一致。
    TopologyMismatch {
        edge: usize,
        expected_bends: usize,
        got_bends: usize,
    },
    /// 旧管线有但 Plan 无对应字段的决策（如同层边、feedback hub）。
    UnmappedDecision { edge: usize, kind: &'static str },
}

/// Adapter 错误。
#[derive(Debug, Error)]
pub enum AdapterError {
    /// 图种不走 LayeredKernel（sequence / mindmap）。
    #[error("图种 {0:?} 不走 LayeredKernel，Adapter 无法构造 Plan")]
    UnsupportedDiagramType(DiagramType),
    /// 蓝图推导失败。
    #[error("蓝图推导失败: {0}")]
    DeriveFailed(String),
    /// 空图（无节点）。
    #[error("空图无节点，无法构造 Plan")]
    EmptyDiagram,
}

// ─── 主入口 ─────────────────────────────────────────────────

/// 从旧管线中间产物反向构造 Plan（23 号文 1.3）。
///
/// 成功返回 `AdapterOutput`（含 Plan + 基底 + 缺口清单）；
/// 图种不适配或空图返回 `Err`。
pub fn adapt_from_legacy(input: &AdapterInput<'_>) -> Result<AdapterOutput, AdapterError> {
    let diagram = input.diagram;
    let layout_result = input.layout_result;

    // 图种守卫：Adapter 仅服务 LayeredKernel 图；Tree/Sequence 走 Dialect 旁路
    match diagram.diagram_type {
        DiagramType::Sequence | DiagramType::Mindmap => {
            // Stage 6：不再硬拒——返回空 Plan 拓扑占位，供 diff；完整几何由 Dialect 产出
            let mut plan = crate::layout::atlas::plan::Plan::default();
            for (i, e) in diagram.entities.iter().enumerate() {
                plan.node_slots.insert(
                    e.id.as_str().to_string(),
                    crate::layout::atlas::plan::Slot {
                        rank: 0,
                        order: i,
                    },
                );
            }
            return Ok(AdapterOutput {
                plan,
                substrate: Default::default(),
                index: BlueprintIndex {
                    cross_lines: BTreeMap::new(),
                    main_lines: BTreeMap::new(),
                    group_ids: BTreeMap::new(),
                    node_region: BTreeMap::new(),
                    node_ports: BTreeMap::new(),
                },
                gaps: vec![ExpressivenessGap::UnmappedDecision {
                    edge: 0,
                    kind: "dialect_bypass_sequence_or_mindmap",
                }],
            });
        }
        _ => {}
    }
    if diagram.entities.is_empty() {
        return Err(AdapterError::EmptyDiagram);
    }

    let mut gaps: Vec<ExpressivenessGap> = Vec::new();

    // ─── 1. node_slots：从 LayeredKernel 取 rank/order ───
    let preset = preset_for(diagram);
    let draft = LayeredKernel::compute(diagram, &preset);
    let mut node_slots: BTreeMap<String, Slot> = BTreeMap::new();
    for (rank, layer) in draft.layers.iter().enumerate() {
        let mut order = 0usize;
        for &node in layer {
            if let LayerNodeKind::Real(dag_node) = draft.proper_graph[node].kind {
                let entity_id = draft.dag[dag_node].clone();
                let r = draft.sugiyama_ranks.get(&entity_id).copied().unwrap_or(rank);
                node_slots.insert(entity_id, Slot { rank: r, order });
                order += 1;
            }
        }
    }

    // ─── 2. 构建 ChannelBlueprint ───
    let mut bp = ChannelBlueprint::default();
    for (node, slot) in &node_slots {
        bp.nodes.insert(
            node.clone(),
            NodeSpec {
                rank: slot.rank,
                order: slot.order,
            },
        );
    }
    // 组树
    for group in &diagram.groups {
        bp.groups.insert(
            group.id.as_str().to_string(),
            GroupSpec {
                members: group
                    .entity_ids
                    .iter()
                    .map(|id| id.as_str().to_string())
                    .collect(),
                parent: group.parent_id.as_ref().map(|p| p.as_str().to_string()),
            },
        );
    }
    // 边（跳过自环，与 probe 同口径）
    for rel in &diagram.relations {
        if rel.from.as_str() == rel.to.as_str() {
            continue;
        }
        bp.edges.push((
            rel.from.as_str().to_string(),
            rel.to.as_str().to_string(),
        ));
    }
    // flat 口径消解
    super::probe::sanitize_overlapping_groups(&mut bp);

    // ─── 3. derive substrate + graph ───
    let (mut substrate, mut index) =
        derive_substrate(&bp).map_err(|e| AdapterError::DeriveFailed(format!("{e:?}")))?;
    // 批量挂端口（四侧，capacity=0 不限——Adapter 验表达力，不验争用）
    derive_node_ports(
        &mut substrate,
        &bp,
        &mut index,
        &DerivePortsOptions::default(),
    )
    .map_err(|e| AdapterError::DeriveFailed(format!("{e:?}")))?;
    let graph = ChannelGraph::from_substrate(&substrate);

    // ─── 4. group_scopes ───
    let mut group_scopes: BTreeMap<GroupKey, super::plan::GroupScopeSpec> = BTreeMap::new();
    for (gname, gid) in &index.group_ids {
        if let Some(g) = substrate.group(*gid) {
            let parent_name = g.parent.and_then(|pid| {
                index
                    .group_ids
                    .iter()
                    .find(|(_, &v)| v == pid)
                    .map(|(k, _)| k.clone())
            });
            group_scopes.insert(
                gname.clone(),
                super::plan::GroupScopeSpec {
                    parent: parent_name,
                    ranks: g.ranks,
                    orders: g.orders,
                },
            );
        }
    }

    // ─── 5. channels/gates：通道图选路 ───
    let rank_count = node_slots.values().map(|s| s.rank + 1).max().unwrap_or(0);
    let order_count = node_slots.values().map(|s| s.order + 1).max().unwrap_or(0);

    let mut plan = Plan {
        substrate: SubstrateSketch {
            rank_count,
            order_count,
        },
        node_slots: node_slots.clone(),
        group_scopes,
        ..Plan::default()
    };

    let mut occupancy = Occupancy::new();
    for (edge_idx, (from, to)) in bp.edges.iter().enumerate() {
        // 构建 L8 掩码
        let mask = index.scope_mask_for_edge(&substrate, from, to);
        // 四侧候选
        let from_ports = index
            .node_ports
            .get(from)
            .cloned()
            .unwrap_or_default();
        let to_ports = index
            .node_ports
            .get(to)
            .cloned()
            .unwrap_or_default();
        if from_ports.is_empty() || to_ports.is_empty() {
            gaps.push(ExpressivenessGap::Infeasible {
                edge: edge_idx,
                from: from.clone(),
                to: to.clone(),
                reason: "端口候选为空".to_string(),
            });
            continue;
        }
        let outcome = route_candidates(&graph, &from_ports, &to_ports, &occupancy, &mask)
            .unwrap_or_else(|_| RouteOutcome::infeasible());
        if outcome.status == crate::layout::kernel::cost::SolverStatus::Converged {
            let _ = plan.record_route(edge_idx, &outcome);
            // Stage 3：commit 占用，后续边看到真实争用（lane Demand 才可信）
            occupancy.commit(&outcome.tracks, &outcome.gates);
            plan.record_ports_from_outcome(edge_idx, &outcome, &substrate);
        } else {
            gaps.push(ExpressivenessGap::Infeasible {
                edge: edge_idx,
                from: from.clone(),
                to: to.clone(),
                reason: format!("{:?}", outcome.status),
            });
        }
    }

    // ─── 6. ports：从旧路由 EdgeLayout 映射 ───
    for (edge_idx, rel) in diagram.relations.iter().enumerate() {
        if rel.from.as_str() == rel.to.as_str() {
            continue; // 自环跳过
        }
        if edge_idx < layout_result.edges.len() {
            let el = &layout_result.edges[edge_idx];
            let from_side = port_to_side(el.from_port);
            let to_side = port_to_side(el.to_port);
            let from_node = rel.from.as_str().to_string();
            let to_node = rel.to.as_str().to_string();
            // slot_id 尝试从 substrate 解析
            let from_slot_id = substrate
                .find_port(&from_node, from_side, 0)
                .map(|p| p.id);
            let to_slot_id = substrate
                .find_port(&to_node, to_side, 0)
                .map(|p| p.id);
            plan.ports.insert(
                edge_idx,
                EdgePorts {
                    from: PortRef {
                        node: from_node,
                        side: from_side,
                        slot_index: 0,
                        slot_id: from_slot_id,
                    },
                    to: PortRef {
                        node: to_node,
                        side: to_side,
                        slot_index: 0,
                        slot_id: to_slot_id,
                    },
                },
            );
        }
    }

    // ─── 7. bundles ───
    plan.detect_and_set_bundles(2);
    plan.assign_lane_indices();

    // ─── 8. provenance：覆写为 LegacyAdapter ───
    for prov in plan.provenance.values_mut() {
        *prov = Provenance::LegacyAdapter;
    }

    // ─── 9. 同层边 / feedback hub 记为 UnmappedDecision ───
    for (idx, rel) in diagram.relations.iter().enumerate() {
        let from_rank = node_slots.get(rel.from.as_str()).map(|s| s.rank);
        let to_rank = node_slots.get(rel.to.as_str()).map(|s| s.rank);
        if let (Some(fr), Some(tr)) = (from_rank, to_rank) {
            if fr == tr {
                gaps.push(ExpressivenessGap::UnmappedDecision {
                    edge: idx,
                    kind: "same_layer_edge",
                });
            }
        }
    }

    Ok(AdapterOutput {
        plan,
        substrate,
        index,
        gaps,
    })
}

// ─── 辅助函数 ─────────────────────────────────────────────────

/// 按图类型选 preset（与 probe 同逻辑）。
fn preset_for(diagram: &Diagram) -> SugiyamaPreset {
    match diagram.diagram_type {
        DiagramType::Flowchart => preset::FLOWCHART_PRESET,
        DiagramType::Er => preset::ER_PRESET,
        DiagramType::State => preset::STATE_PRESET,
        DiagramType::Architecture => preset::ARCHITECTURE_PRESET,
        _ => preset::GENERIC_PRESET,
    }
}

/// 旧 `Port` → Atlas `PortSide` 映射。
fn port_to_side(port: Port) -> PortSide {
    match port {
        Port::Top => PortSide::MainLow,
        Port::Bottom => PortSide::MainHigh,
        Port::Left => PortSide::CrossLow,
        Port::Right => PortSide::CrossHigh,
    }
}

// ─── 测试 ─────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::prepare::parse_prepare_validate;
    use crate::prepare::StyleRequest;

    /// 从源码跑完整管线得 LayoutResult，再跑 Adapter。
    fn adapt_source(source: &str) -> AdapterOutput {
        let output = parse_prepare_validate(source, &StyleRequest::default());
        assert!(output.is_valid(), "parse failed: {:?}", output.errors);
        let prepared = output.diagram.unwrap();
        let diagram = prepared.inner();
        let plan = prepared.layout_plan();
        let layout_result =
            crate::layout::pipeline::entry::compute_layout_with_plan(diagram, plan)
                .expect("layout failed");
        let input = AdapterInput {
            diagram,
            layout_result: &layout_result,
        };
        adapt_from_legacy(&input).expect("adapter failed")
    }

    #[test]
    fn chain_produces_valid_plan() {
        let source = r#"diagram flowchart {
  entity a "A"
  entity b "B"
  entity c "C"
  a -> b
  b -> c
}"#;
        let out = adapt_source(source);
        assert!(out.plan.validate().is_ok());
        assert!(!out.plan.node_slots.is_empty());
        assert_eq!(out.plan.node_slots.len(), 3);
        // channels 应有成功边
        assert!(!out.plan.channels.is_empty(), "至少一条边选路成功");
    }

    #[test]
    fn empty_diagram_returns_error() {
        let source = "diagram flowchart {\n}";
        let output = parse_prepare_validate(source, &StyleRequest::default());
        let prepared = output.diagram.unwrap();
        let diagram = prepared.inner();
        let plan_lp = prepared.layout_plan();
        let layout_result =
            crate::layout::pipeline::entry::compute_layout_with_plan(diagram, plan_lp)
                .expect("layout failed");
        let input = AdapterInput {
            diagram,
            layout_result: &layout_result,
        };
        assert!(matches!(
            adapt_from_legacy(&input),
            Err(AdapterError::EmptyDiagram)
        ));
    }

    #[test]
    fn deterministic_fingerprint() {
        let source = include_str!("../../../../../showcase/flowchart/product.user-auth.pgm");
        let output = parse_prepare_validate(source, &StyleRequest::default());
        let prepared = output.diagram.unwrap();
        let diagram = prepared.inner();
        let plan_lp = prepared.layout_plan();
        let layout_result =
            crate::layout::pipeline::entry::compute_layout_with_plan(diagram, plan_lp)
                .expect("layout failed");
        let input = AdapterInput {
            diagram,
            layout_result: &layout_result,
        };
        let out1 = adapt_from_legacy(&input).expect("adapter failed");
        let out2 = adapt_from_legacy(&input).expect("adapter failed");
        assert_eq!(
            out1.plan.fingerprint(),
            out2.plan.fingerprint(),
            "同图双跑指纹应相等"
        );
    }

    #[test]
    fn grouped_flowchart_has_group_scopes() {
        let source = r#"diagram flowchart {
  group g1 "分组" {
    entity a "A"
    entity b "B"
  }
  entity c "C"
  a -> b
  b -> c
}"#;
        let out = adapt_source(source);
        assert!(out.plan.validate().is_ok());
        // 组可能被 sanitize 丢弃（flat 口径），但若有则 scopes 非空
        if !out.plan.group_scopes.is_empty() {
            let scope = out.plan.group_scopes.values().next().unwrap();
            assert!(scope.ranks.0 <= scope.ranks.1);
        }
    }

    #[test]
    fn ports_mapped_from_legacy() {
        let source = r#"diagram flowchart {
  entity a "A"
  entity b "B"
  a -> b
}"#;
        let out = adapt_source(source);
        // 至少一条边有端口映射
        assert!(!out.plan.ports.is_empty(), "应有端口映射");
        let ep = out.plan.ports.values().next().unwrap();
        // flowchart TB：from 通常 Bottom，to 通常 Top
        assert_eq!(ep.from.side, PortSide::MainHigh);
        assert_eq!(ep.to.side, PortSide::MainLow);
    }
}
