//! 可行率探针门面（23 号文 Stage 1 Legacy Adapter 的最小前置）：
//! 把 [`Diagram`] 直接变成 [`ChannelBlueprint`]，桥接私有的分层布局内核
//! （[`LayeredKernel`]，`pub(in crate::layout)`）与公开的通道图推导契约。
//!
//! **口径**：所有图统一走 `LayeredKernel::compute` 取 flat rank/order 网格——
//! 即「假设整图走单层 Sugiyama」时的 rank/order。divide-conquer（有组的 flowchart）
//! 与 two_phase（有组的 architecture）在真实管线里走别的路径，本门面不镜像，
//! 由探针报告标注、解读时留意。这是粒度可行性实验，不是生产接线。
//!
//! 探针（`plotgram-eval` 的 `atlas_probe` bin）据此在 product 集上测量
//! 「相 I 单边选路成功率」，回答 22 号文风险台账第一条（通道图粒度是否够细）。

use super::channel::{ChannelBlueprint, GroupSpec, NodeSpec};
use crate::ast::Diagram;
use crate::layout::kernel::layered::graph::LayerNodeKind;
use crate::layout::kernel::layered::layered_kernel::LayeredKernel;
use crate::layout::kernel::layered::preset;
use crate::layout::kernel::layered::preset::SugiyamaPreset;
use crate::types::DiagramType;
use std::collections::{BTreeMap, BTreeSet};

/// 组边界 gate 的默认**诊断扫描**容量（27 号文 L4 后 gate 容量不再是生产输入）。
///
/// 生产路径恒 `GateCapacity::Unbounded`（穿越数是输出不是约束）；本常量仅供
/// 探针的 `gate_capacity_override` 诊断扫描作基准值，量化固定容量下的争用。
pub const DEFAULT_GATE_CAPACITY: u32 = 4;

/// 按图类型选 preset（与各 recipe 的 compile 阶段一致）。
fn preset_for(diagram: &Diagram) -> SugiyamaPreset {
    match diagram.diagram_type {
        DiagramType::Flowchart => preset::FLOWCHART_PRESET,
        DiagramType::Er => preset::ER_PRESET,
        DiagramType::State => preset::STATE_PRESET,
        DiagramType::Architecture => preset::ARCHITECTURE_PRESET,
        // sequence / mindmap / 自定义不走 LayeredKernel，用通用 preset 兜底
        // （探针会因图结构不适配而成功率偏低，报告中按类型分组可见）。
        _ => preset::GENERIC_PRESET,
    }
}

/// 从 [`Diagram`] 推导 [`ChannelBlueprint`]（无坐标，只有 rank/order + 组树 + 边）。
///
/// 空图返回空蓝图。rank 取自 `sugiyama_ranks`，order 取自排序后各层内 Real 节点
/// 的序号（跳过 dummy，得到紧凑网格）。
pub fn derive_channel_blueprint(diagram: &Diagram) -> ChannelBlueprint {
    let mut bp = ChannelBlueprint::default();
    if diagram.entities.is_empty() {
        return bp;
    }

    let preset = preset_for(diagram);
    let draft = LayeredKernel::compute(diagram, &preset);

    // order：遍历排序后各层，Real 节点按层内序号紧凑编号（跳过 dummy）。
    // layers[rank] 内可能混有 dummy（长边拆段），只取 Real 映射回 entity_id。
    for (rank, layer) in draft.layers.iter().enumerate() {
        let mut order = 0usize;
        for &node in layer {
            if let LayerNodeKind::Real(dag_node) = draft.proper_graph[node].kind {
                let entity_id = draft.dag[dag_node].clone();
                // rank 以 sugiyama_ranks 为准（与层索引一致，但显式取语义量）。
                let r = draft.sugiyama_ranks.get(&entity_id).copied().unwrap_or(rank);
                bp.nodes.insert(entity_id, NodeSpec { rank: r, order });
                order += 1;
            }
        }
    }

    // 组树：成员 + 父组（None = 顶层直属）。
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

    // 边：取声明端点。方向（含 FAS 反转的回边）由 edge_port_tracks 按 rank 处理。
    // 自环跳过（L7-T5 定案）：channel 不建模同节点边（route 显式 Infeasible），
    // 生产管线自环走节点旁小环、不经通道，故蓝图口径同样排除。
    // 探针报告以 relations 与 bp.edges 的差值披露排除数。
    for rel in &diagram.relations {
        if rel.from.as_str() == rel.to.as_str() {
            continue;
        }
        bp.edges.push((
            rel.from.as_str().to_string(),
            rel.to.as_str().to_string(),
        ));
    }

    // flat 口径消解：交叠/不纯的组丢弃（derive 构建期拒绝非嵌套树基底）。
    sanitize_overlapping_groups(&mut bp);

    bp
}

/// flat 口径消解组矩形病态：丢弃破坏切割模型前提的组（及其子树）——
/// 与 derive 构建期两条拒绝规则一一对应：
///
/// 1. **不纯**（`ForeignNodeInGroupRect`）：包围盒（后代节点闭区间，与 derive
///    同规则）内含非后代节点 → 该节点宿主段 scope 归属错乱，其边必爆 A2/A3。
/// 2. **交叠**（`OverlappingGroups`）：与已保留的无祖先关系组矩形相交 → 按
///    （深度, 组名）升序保留先者、丢后者。
///
/// LayeredKernel 的 flat 网格不做组感知布局（不镜像 divide-conquer /
/// two_phase），组成员的 rank/order 可能散布，两种病态都会出现。丢组只影响
/// 探针口径（该组约束变松，A2 检验力度按图打折），探针报告以「丢交叠组」
/// 标注受影响图；生产接线（Legacy Adapter）将镜像真实分区布局，不经此
/// 路径。全程 BTreeMap/排序遍历，确定性。
fn sanitize_overlapping_groups(bp: &mut ChannelBlueprint) {
    if bp.groups.is_empty() {
        return;
    }
    let depth = |g: &str| -> usize {
        let mut d = 0;
        let mut cur = bp.groups.get(g).and_then(|s| s.parent.clone());
        while let Some(p) = cur {
            d += 1;
            cur = bp.groups.get(&p).and_then(|s| s.parent.clone());
        }
        d
    };
    let is_ancestor = |anc: &str, g: &str| -> bool {
        let mut cur = bp.groups.get(g).and_then(|s| s.parent.clone());
        while let Some(p) = cur {
            if p == anc {
                return true;
            }
            cur = bp.groups.get(&p).and_then(|s| s.parent.clone());
        }
        false
    };
    let mut children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (g, spec) in &bp.groups {
        if let Some(p) = &spec.parent {
            children.entry(p.clone()).or_default().push(g.clone());
        }
    }
    // 后代节点包围盒 + 后代集；空子树不参与判定（留给 derive 报 EmptyGroup）。
    let mut rect: BTreeMap<String, (usize, usize, usize, usize)> = BTreeMap::new();
    let mut desc: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for g in bp.groups.keys() {
        let (mut r0, mut r1) = (usize::MAX, 0usize);
        let (mut o0, mut o1) = (usize::MAX, 0usize);
        let mut members: BTreeSet<String> = BTreeSet::new();
        let mut stack = vec![g.clone()];
        while let Some(cur) = stack.pop() {
            if let Some(spec) = bp.groups.get(&cur) {
                for m in &spec.members {
                    if let Some(ns) = bp.nodes.get(m) {
                        members.insert(m.clone());
                        r0 = r0.min(ns.rank);
                        r1 = r1.max(ns.rank);
                        o0 = o0.min(ns.order);
                        o1 = o1.max(ns.order);
                    }
                }
            }
            if let Some(kids) = children.get(&cur) {
                stack.extend(kids.iter().cloned());
            }
        }
        if !members.is_empty() {
            rect.insert(g.clone(), (r0, r1, o0, o1));
            desc.insert(g.clone(), members);
        }
    }
    let mut order: Vec<String> = bp.groups.keys().cloned().collect();
    order.sort_by_key(|g| (depth(g), g.clone()));
    let mut dropped: BTreeSet<String> = BTreeSet::new();
    let mut kept: Vec<String> = Vec::new();
    for g in &order {
        // 父被丢 → 子树整体丢（parent 指针不可悬空）。
        if let Some(p) = bp.groups[g].parent.as_ref() {
            if dropped.contains(p) {
                dropped.insert(g.clone());
                continue;
            }
        }
        let (impure, conflict) = match rect.get(g) {
            None => (false, false),
            Some(&(r0, r1, o0, o1)) => {
                // 不纯：矩形内含非后代节点（宿主段 scope 归属错乱）。
                let members = &desc[g];
                let impure = bp.nodes.iter().any(|(n, ns)| {
                    r0 <= ns.rank
                        && ns.rank <= r1
                        && o0 <= ns.order
                        && ns.order <= o1
                        && !members.contains(n)
                });
                // 交叠：与已保留的无祖先关系组矩形相交。
                let conflict = kept.iter().any(|k| {
                    if is_ancestor(k, g) || is_ancestor(g, k) {
                        return false;
                    }
                    match rect.get(k) {
                        Some(&(kr0, kr1, ko0, ko1)) => {
                            r0 <= kr1 && kr0 <= r1 && o0 <= ko1 && ko0 <= o1
                        }
                        None => false,
                    }
                });
                (impure, conflict)
            }
        };
        if impure || conflict {
            dropped.insert(g.clone());
        } else {
            kept.push(g.clone());
        }
    }
    for g in &dropped {
        bp.groups.remove(g);
    }
    // 级联清理：子树被丢空的残留组（仅由子组撑起的容器组）也丢——否则
    // derive 对空组报 EmptyGroup，整图误记不可表达。自叶向根不动点。
    loop {
        let mut has_child: BTreeSet<&str> = BTreeSet::new();
        for spec in bp.groups.values() {
            if let Some(p) = &spec.parent {
                has_child.insert(p.as_str());
            }
        }
        let empty: Vec<String> = bp
            .groups
            .iter()
            .filter(|(g, spec)| {
                !has_child.contains(g.as_str())
                    && !spec.members.iter().any(|m| bp.nodes.contains_key(m))
            })
            .map(|(g, _)| g.clone())
            .collect();
        if empty.is_empty() {
            break;
        }
        for g in empty {
            bp.groups.remove(&g);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ArrowType, AttributeMap, Entity, Identifier, Relation, SourceInfo, Span};
    use crate::prepare::StyleRequest;

    fn flowchart(entities: Vec<&str>, relations: Vec<(&str, &str)>) -> Diagram {
        let span = Span::dummy();
        Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities: entities
                .into_iter()
                .map(|id| Entity {
                    id: Identifier::new_unchecked(id),
                    label: id.to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span,
                })
                .collect(),
            relations: relations
                .into_iter()
                .map(|(from, to)| Relation {
                    from: Identifier::new_unchecked(from),
                    to: Identifier::new_unchecked(to),
                    arrow: ArrowType::Active,
                    label: None,
                    head_label: None,
                    tail_label: None,
                    attributes: AttributeMap::default(),
                    span,
                })
                .collect(),
            groups: vec![],
            constraints: vec![],
            style_decls: vec![],
            source_info: SourceInfo { file: None, line_count: 1 },
            ..Default::default()
        }
    }

    #[test]
    fn sanitize_drops_pathological_groups_deterministically() {
        // 病态一（不纯）：组外节点 c 落在 alpha 包围盒内部 → alpha 丢弃。
        let mut bp = ChannelBlueprint::default();
        bp.nodes.insert("a".into(), NodeSpec { rank: 0, order: 0 });
        bp.nodes.insert("b".into(), NodeSpec { rank: 5, order: 2 });
        bp.nodes.insert("c".into(), NodeSpec { rank: 3, order: 1 });
        bp.groups.insert(
            "alpha".into(),
            GroupSpec {
                members: vec!["a".into(), "b".into()],
                parent: None,
            },
        );
        sanitize_overlapping_groups(&mut bp);
        assert!(bp.groups.is_empty(), "不纯组丢弃");

        // 病态二（纯交叠）：两组都纯（交叠区无节点）但矩形相交 →
        // （深度, 组名）序保留先者 p，丢 q 及其子树 q_sub；分离的 r 保留。
        let mut bp2 = ChannelBlueprint::default();
        bp2.nodes.insert("p1".into(), NodeSpec { rank: 0, order: 0 });
        bp2.nodes.insert("p2".into(), NodeSpec { rank: 3, order: 0 });
        bp2.nodes.insert("p3".into(), NodeSpec { rank: 0, order: 3 });
        bp2.nodes.insert("q1".into(), NodeSpec { rank: 2, order: 4 });
        bp2.nodes.insert("q2".into(), NodeSpec { rank: 4, order: 2 });
        bp2.nodes.insert("r1".into(), NodeSpec { rank: 9, order: 9 });
        bp2.groups.insert(
            "p".into(),
            GroupSpec {
                members: vec!["p1".into(), "p2".into(), "p3".into()],
                parent: None,
            },
        );
        bp2.groups.insert(
            "q".into(),
            GroupSpec {
                members: vec!["q1".into()],
                parent: None,
            },
        );
        bp2.groups.insert(
            "q_sub".into(),
            GroupSpec {
                members: vec!["q2".into()],
                parent: Some("q".into()),
            },
        );
        bp2.groups.insert(
            "r".into(),
            GroupSpec {
                members: vec!["r1".into()],
                parent: None,
            },
        );
        sanitize_overlapping_groups(&mut bp2);
        assert!(bp2.groups.contains_key("p"), "组名序先者保留");
        assert!(!bp2.groups.contains_key("q"), "交叠后者丢弃");
        assert!(!bp2.groups.contains_key("q_sub"), "被丢组的子树整体丢");
        assert!(bp2.groups.contains_key("r"), "分离组不受影响");

        // 病态三（级联清理）：无直属成员的容器组自身纯净（后代集覆盖全部
        // 矩内节点），但两子组矩形互含对方节点均不纯被丢 → 容器成空组，
        // 自叶向根连锁丢弃（否则 derive 报 EmptyGroup 整图不可表达）。
        let mut bp3 = ChannelBlueprint::default();
        bp3.nodes.insert("a1".into(), NodeSpec { rank: 0, order: 0 });
        bp3.nodes.insert("a2".into(), NodeSpec { rank: 1, order: 2 });
        bp3.nodes.insert("b1".into(), NodeSpec { rank: 1, order: 1 });
        bp3.nodes.insert("b2".into(), NodeSpec { rank: 2, order: 2 });
        bp3.groups.insert(
            "holder".into(),
            GroupSpec { members: vec![], parent: None },
        );
        bp3.groups.insert(
            "kid_a".into(),
            GroupSpec {
                members: vec!["a1".into(), "a2".into()], // 矩形含 b1 → 不纯
                parent: Some("holder".into()),
            },
        );
        bp3.groups.insert(
            "kid_b".into(),
            GroupSpec {
                members: vec!["b1".into(), "b2".into()], // 矩形含 a2 → 不纯
                parent: Some("holder".into()),
            },
        );
        sanitize_overlapping_groups(&mut bp3);
        assert!(!bp3.groups.contains_key("kid_a"), "不纯子组丢弃");
        assert!(!bp3.groups.contains_key("kid_b"), "不纯子组丢弃");
        assert!(!bp3.groups.contains_key("holder"), "被丢空的容器组级联丢弃");

        let (s, _idx) =
            crate::layout::atlas::channel::derive_substrate(&bp2).expect("消解后必可 derive");
        assert!(s.verify_no_group_penetration().is_empty(), "L6 全通过");
    }

    #[test]
    fn empty_diagram_yields_empty_blueprint() {
        let d = flowchart(vec![], vec![]);
        let bp = derive_channel_blueprint(&d);
        assert!(bp.nodes.is_empty());
        assert!(bp.edges.is_empty());
    }

    #[test]
    fn chain_produces_monotonic_ranks_and_edges() {
        // a → b → c：三个节点应分到递增 rank，order 紧凑，边全保留。
        let d = flowchart(vec!["a", "b", "c"], vec![("a", "b"), ("b", "c")]);
        let bp = derive_channel_blueprint(&d);

        assert_eq!(bp.nodes.len(), 3);
        let ra = bp.nodes["a"].rank;
        let rb = bp.nodes["b"].rank;
        let rc = bp.nodes["c"].rank;
        assert!(ra < rb && rb < rc, "rank 应单调：{ra} < {rb} < {rc}");
        assert_eq!(bp.edges.len(), 2);
        assert!(bp.edges.contains(&("a".to_string(), "b".to_string())));
        assert!(bp.edges.contains(&("b".to_string(), "c".to_string())));
    }

    #[test]
    fn self_loop_edges_are_excluded_from_blueprint() {
        // a → a 自环不进蓝图（L7-T5：channel 不建模，生产走节点旁小环）。
        let d = flowchart(vec!["a", "b"], vec![("a", "a"), ("a", "b")]);
        let bp = derive_channel_blueprint(&d);
        assert_eq!(bp.edges, vec![("a".to_string(), "b".to_string())]);
    }

    #[test]
    fn same_rank_siblings_get_compact_orders() {
        // a → {b, c}：b、c 同 rank，order 应为 0、1（紧凑，无 dummy 空洞）。
        let d = flowchart(vec!["a", "b", "c"], vec![("a", "b"), ("a", "c")]);
        let bp = derive_channel_blueprint(&d);

        let b = &bp.nodes["b"];
        let c = &bp.nodes["c"];
        assert_eq!(b.rank, c.rank, "b、c 应同 rank");
        let mut orders = [b.order, c.order];
        orders.sort();
        assert_eq!(orders, [0, 1], "同 rank order 应紧凑 0、1");
    }

    #[test]
    fn blueprint_is_deterministic() {
        let source = include_str!("../../../../../showcase/flowchart/product.user-auth.pgm");
        let output = crate::pipeline::parse_prepare_validate(source, &StyleRequest::default());
        let prepared = output.diagram.expect("valid diagram");

        let b1 = derive_channel_blueprint(prepared.inner());
        let b2 = derive_channel_blueprint(prepared.inner());
        assert_eq!(b1.nodes, b2.nodes);
        assert_eq!(b1.edges, b2.edges);
        assert_eq!(b1.groups.len(), b2.groups.len());
    }
}
