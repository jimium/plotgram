//! `PreparedRoutingInput` + `prepare()` + problem signature（doc16 §4.1 / Slice B）。
//!
//! 目标：router 不再直接解析完整 `Diagram`/`LayoutResult`，而是消费一份**只读、已编译**
//! 的路由输入。Slice B：router 经 `PreparedRoutingInput` 消费只读快照，类型上不可能
//! 修改 nodes/groups。
//!
//! 退出判据：
//! - problem signature 可完整描述一次 route run（本模块 [`PreparedRoutingInput::problem_signature`]）。
//! - Kernel/prepare 不读取 `DiagramType`（本模块只读 relations/groups/nodes 几何与声明语义）。

use super::contract::RoutingContract;
use super::stable_edge::StableEdgeStore;
use crate::ast::Diagram;
use crate::layout::routing::config::RoutingConfig;
use crate::layout::routing::coordinator::FrozenNodeProduct;
use crate::layout::types::{GroupLayout, LayoutHints, NodeLayout};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// 路由画布尺寸（只读）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoutingCanvas {
    pub width: f64,
    pub height: f64,
}

/// 已编译的只读路由输入（Slice B）。
///
/// 借用 `FrozenNodeProduct`（nodes/groups 只读快照）+ `Diagram`（声明语义）+
/// `LayoutHints`（布局 hints），类型上不可能修改 nodes/groups。
pub struct PreparedRoutingInput<'a> {
    /// 只读节点/分组快照（直接借用 FrozenNodeProduct）。
    pub frozen: &'a FrozenNodeProduct,
    /// 声明语义（relations / entities / groups / diagram_type）。
    pub diagram: &'a Diagram,
    /// 确定性边存储。
    pub edges: StableEdgeStore,
    /// 语义契约。
    pub contract: RoutingContract,
    /// 生效方向字符串（如 `"top-to-bottom"`）；空串表示该布局不消费方向。
    pub direction: String,
    /// 画布尺寸。
    pub canvas: RoutingCanvas,
    /// 生效的路由族/风格名（`straight`/`orthogonal`/…）。
    pub family: String,
    /// 正式路由配置。
    pub config: RoutingConfig,
    /// 布局 hints（sugiyama_ranks / space_budget 等，只读借用）。
    pub hints: &'a LayoutHints,
}

impl<'a> PreparedRoutingInput<'a> {
    /// 从冻结快照 + diagram 声明语义编译路由输入。
    ///
    /// 不读取 `DiagramType`。`direction` / `family` / `config` 由调用方传入。
    pub fn prepare(
        frozen: &'a FrozenNodeProduct,
        diagram: &'a Diagram,
        hints: &'a LayoutHints,
        direction: &str,
        family: &str,
        config: RoutingConfig,
        canvas: RoutingCanvas,
    ) -> Self {
        let edges = StableEdgeStore::from_diagram(diagram);
        let contract = RoutingContract::compile(&edges);
        Self {
            frozen,
            diagram,
            edges,
            contract,
            direction: direction.to_string(),
            canvas,
            family: family.to_string(),
            config,
            hints,
        }
    }

    /// 便捷方法：只读节点 map。
    pub fn nodes(&self) -> &std::collections::HashMap<String, NodeLayout> {
        &self.frozen.nodes
    }

    /// 便捷方法：只读分组 map。
    pub fn groups(&self) -> &std::collections::HashMap<String, GroupLayout> {
        &self.frozen.groups
    }

    /// 确定性 problem signature（u64）：可完整描述一次 route run 的输入。
    ///
    /// 覆盖：family、direction、canvas、config、节点几何（按 id 升序）、分组几何（按 id 升序）、
    /// 边（id/from/to/arrow/label 标志）、逐边角色 token。f64 用 `to_bits` 保证确定性哈希。
    /// 严禁依赖 HashMap 迭代顺序（AGENTS.md §2）。
    pub fn problem_signature(&self) -> u64 {
        let mut h = DefaultHasher::new();

        self.family.hash(&mut h);
        self.direction.hash(&mut h);
        self.canvas.width.to_bits().hash(&mut h);
        self.canvas.height.to_bits().hash(&mut h);
        // config 字段纳入 signature
        format!("{:?}", self.config).hash(&mut h);

        // 节点几何：按 id 升序
        let mut node_entries: Vec<(&String, &NodeLayout)> = self.frozen.nodes.iter().collect();
        node_entries.sort_by(|a, b| a.0.cmp(b.0));
        for (id, n) in node_entries {
            id.hash(&mut h);
            n.x.to_bits().hash(&mut h);
            n.y.to_bits().hash(&mut h);
            n.width.to_bits().hash(&mut h);
            n.height.to_bits().hash(&mut h);
        }

        // 分组几何：按 id 升序
        let mut group_entries: Vec<(&String, &GroupLayout)> = self.frozen.groups.iter().collect();
        group_entries.sort_by(|a, b| a.0.cmp(b.0));
        for (id, g) in group_entries {
            id.hash(&mut h);
            g.x.to_bits().hash(&mut h);
            g.y.to_bits().hash(&mut h);
            g.width.to_bits().hash(&mut h);
            g.height.to_bits().hash(&mut h);
        }

        // 边：按 StableEdgeId（即声明序）
        for e in self.edges.iter() {
            e.id.index().hash(&mut h);
            e.from.hash(&mut h);
            e.to.hash(&mut h);
            format!("{:?}", e.arrow).hash(&mut h);
            e.has_mid_label.hash(&mut h);
            e.has_head_label.hash(&mut h);
            e.has_tail_label.hash(&mut h);
        }

        // 逐边角色 token（确定性）
        for tok in self.contract.role_signature_tokens() {
            tok.hash(&mut h);
        }

        h.finish()
    }

    /// 人类可读的 signature 摘要（诊断日志用）。
    pub fn signature_describe(&self) -> String {
        format!(
            "family={} dir={} edges={} nodes={} groups={} self_loops={} parallel_groups={} sig={:016x}",
            self.family,
            if self.direction.is_empty() { "-" } else { &self.direction },
            self.edges.len(),
            self.frozen.nodes.len(),
            self.frozen.groups.len(),
            self.contract.topology.self_loop_count,
            self.contract.topology.parallel_group_count,
            self.problem_signature(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ArrowType, Identifier, Relation, SourceInfo, Span, AttributeMap};
    use crate::types::DiagramType;
    use crate::layout::types::LayoutResult;
    use std::collections::HashMap;

    fn rel(from: &str, to: &str) -> Relation {
        Relation {
            from: Identifier::new_unchecked(from),
            to: Identifier::new_unchecked(to),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        }
    }

    fn diagram() -> Diagram {
        let mut d = Diagram::new(DiagramType::Flowchart, SourceInfo::default());
        d.relations = vec![rel("a", "b"), rel("b", "c")];
        d
    }

    fn frozen() -> FrozenNodeProduct {
        let mut nodes = HashMap::new();
        nodes.insert("a".into(), NodeLayout { x: 0.0, y: 0.0, width: 40.0, height: 30.0, ..Default::default() });
        nodes.insert("b".into(), NodeLayout { x: 100.0, y: 0.0, width: 40.0, height: 30.0, ..Default::default() });
        nodes.insert("c".into(), NodeLayout { x: 200.0, y: 0.0, width: 40.0, height: 30.0, ..Default::default() });
        let result = LayoutResult {
            nodes,
            groups: HashMap::new(),
            edges: Vec::new(),
            total_width: 300.0,
            total_height: 100.0,
            hints: Default::default(),
        };
        FrozenNodeProduct::capture(&result)
    }

    fn canvas() -> RoutingCanvas {
        RoutingCanvas { width: 300.0, height: 100.0 }
    }

    #[test]
    fn signature_is_deterministic() {
        let d = diagram();
        let f = frozen();
        let hints = LayoutHints::default();
        let cfg = RoutingConfig::default();
        let a = PreparedRoutingInput::prepare(&f, &d, &hints, "top-to-bottom", "orthogonal", cfg, canvas()).problem_signature();
        let b = PreparedRoutingInput::prepare(&f, &d, &hints, "top-to-bottom", "orthogonal", cfg, canvas()).problem_signature();
        assert_eq!(a, b);
    }

    #[test]
    fn signature_reflects_family_and_direction() {
        let d = diagram();
        let f = frozen();
        let hints = LayoutHints::default();
        let cfg = RoutingConfig::default();
        let base = PreparedRoutingInput::prepare(&f, &d, &hints, "top-to-bottom", "orthogonal", cfg, canvas()).problem_signature();
        let diff_family = PreparedRoutingInput::prepare(&f, &d, &hints, "top-to-bottom", "straight", cfg, canvas()).problem_signature();
        let diff_dir = PreparedRoutingInput::prepare(&f, &d, &hints, "left-to-right", "orthogonal", cfg, canvas()).problem_signature();
        assert_ne!(base, diff_family);
        assert_ne!(base, diff_dir);
    }
}
