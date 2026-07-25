//! 富 `RoutingContract`：路由语义契约（doc16 §4.2）。
//!
//! 注意：与既有极简 [`crate::layout::demand::spacing_contract::RoutingContract`]
//! **同名不同物**。后者只表达 port clearance / corridor boost 两个数值；
//! 本类型表达「为什么这样路由」的 typed roles / intents，是路由问题模型的语义输入。
//!
//! ## Slice 1 范围
//!
//! 本切片只从**声明语义**（relations / groups / arrow）确定性抽取可无歧义得到的部分：
//! - `edge_roles`：SelfLoop / ParallelGroup / Forward（默认）。
//! - `circle_membership`：暂全 `None`（circular 语义留待后续 Slice）。
//! - `topology`：结构性计数（供 problem signature 与诊断）。
//! - `label_policy`：沿用现状（声明 merge 去重 + 冻结后定位）。
//!
//! 依赖布局拓扑/rank 的 role（Feedback / CrossScope / Monitor / Business / Pendant）
//! 与 port/transit/corridor/merge intents，**结构位已就位但暂空**，由后续 Slice 填充。
//! `EdgeRole` 一律来自声明语义 / 布局拓扑 / Recipe compiler，**不得从路径点数反猜**（doc16 §4.2）。

use super::stable_edge::{StableEdgeId, StableEdgeStore};
use crate::layout::types::Port;
use serde::Serialize;
use std::collections::BTreeMap;

/// 平行边组标识（相同无序端点对的多条边）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ParallelGroupId(pub usize);

/// 环成员标识（circular 语义占位，Slice 1 未使用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct CircleId(pub usize);

/// 显式声明合并组标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct MergeGroupId(pub usize);

/// 边角色：来自声明语义 / 布局拓扑 / Recipe compiler，不得从几何反猜。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum EdgeRole {
    /// 默认前向边。
    Forward,
    /// 反馈边（依赖 rank，后续 Slice 填充）。
    Feedback,
    /// 同层边（依赖 rank，后续 Slice 填充）。
    SameLayer,
    /// 跨作用域/跨组边（后续 Slice 填充）。
    CrossScope,
    /// 监控边（架构语义，后续 Slice 填充）。
    Monitor,
    /// 业务边（架构语义，后续 Slice 填充）。
    Business,
    /// 挂件边（后续 Slice 填充）。
    Pendant,
    /// 自环。
    SelfLoop,
    /// 平行边组成员。
    ParallelGroup(ParallelGroupId),
}

/// 单条边的角色集合（确定性升序去重）。
#[derive(Debug, Clone, Default, Serialize)]
pub struct EdgeRoleSet {
    roles: Vec<EdgeRole>,
}

impl EdgeRoleSet {
    pub fn insert(&mut self, role: EdgeRole) {
        if let Err(pos) = self.roles.binary_search(&role) {
            self.roles.insert(pos, role);
        }
    }

    pub fn contains(&self, role: &EdgeRole) -> bool {
        self.roles.binary_search(role).is_ok()
    }

    pub fn as_slice(&self) -> &[EdgeRole] {
        &self.roles
    }

    pub fn is_empty(&self) -> bool {
        self.roles.is_empty()
    }
}

/// 端口意图（后续 Slice 由 PortAssignmentSolver 前置填充）。
#[derive(Debug, Clone, Serialize)]
pub struct PortIntent {
    pub edge: StableEdgeId,
    pub prefer_from: Option<Port>,
    pub prefer_to: Option<Port>,
}

/// 跨作用域穿越意图（后续 Slice 填充）。
#[derive(Debug, Clone, Serialize)]
pub struct TransitIntent {
    pub edge: StableEdgeId,
    pub cross_group: bool,
    /// Phase 2：偏好外围通道（feedback / Monitor）；LexA* Q5。
    pub prefer_periphery: bool,
}

/// 走廊资源（后续 Slice 由 ResourceGraph 填充）。
#[derive(Debug, Clone, Serialize)]
pub struct CorridorResource {
    pub id: usize,
    pub position: f64,
}

/// 侧边留白资源（后续 Slice 填充）。
#[derive(Debug, Clone, Serialize)]
pub struct SideGutterResource {
    pub group: String,
    pub side: Port,
    pub width: f64,
}

/// 合并意图（后续 Slice 由 BundleSolver 作为一等 BundleProblem 消费）。
#[derive(Debug, Clone, Serialize)]
pub struct MergeIntent {
    pub group: MergeGroupId,
    pub edges: Vec<StableEdgeId>,
}

/// 标签策略。
#[derive(Debug, Clone, Serialize)]
pub struct LabelPolicy {
    /// 对声明 merge 去重（沿用现状）。
    pub dedupe_declared_merges: bool,
    /// 在 geometry freeze 后统一定位（doc16 §14 终态；Slice 1 记录意图）。
    pub place_after_freeze: bool,
}

impl Default for LabelPolicy {
    fn default() -> Self {
        Self {
            dedupe_declared_merges: true,
            place_after_freeze: true,
        }
    }
}

/// 路由拓扑元数据：结构性计数，供 problem signature 与诊断。
#[derive(Debug, Clone, Default, Serialize)]
pub struct RoutingTopologyMetadata {
    pub edge_count: usize,
    pub self_loop_count: usize,
    pub parallel_group_count: usize,
}

/// 富路由契约。
#[derive(Debug, Clone, Serialize)]
pub struct RoutingContract {
    /// 逐边角色集合，长度与 `StableEdgeStore` 一致，按 `StableEdgeId` 对齐。
    pub edge_roles: Vec<EdgeRoleSet>,
    pub port_intents: Vec<PortIntent>,
    pub transit_intents: Vec<TransitIntent>,
    pub corridors: Vec<CorridorResource>,
    pub side_gutters: Vec<SideGutterResource>,
    pub merge_intents: Vec<MergeIntent>,
    /// 逐边环成员，长度与边一致（Slice 1 全 `None`）。
    pub circle_membership: Vec<Option<CircleId>>,
    pub topology: RoutingTopologyMetadata,
    pub label_policy: LabelPolicy,
}

impl RoutingContract {
    /// 从稳定边存储编译契约。
    ///
    /// Slice 1：确定性抽取 SelfLoop / ParallelGroup / Forward。不读取 `DiagramType`。
    pub fn compile(edges: &StableEdgeStore) -> Self {
        let n = edges.len();

        // 平行边分组：按端点无序 key 聚合。id 按 key 首次出现顺序稳定分配。
        let mut group_of_key: BTreeMap<(String, String), Vec<StableEdgeId>> = BTreeMap::new();
        for e in edges.iter() {
            if e.is_self_loop() {
                continue;
            }
            group_of_key.entry(e.endpoint_key()).or_default().push(e.id);
        }
        // 仅 >1 条边的 key 构成平行组；按 key 升序分配稳定 ParallelGroupId。
        let mut parallel_id_of_edge: BTreeMap<usize, ParallelGroupId> = BTreeMap::new();
        let mut parallel_group_count = 0usize;
        for members in group_of_key.values() {
            if members.len() > 1 {
                let gid = ParallelGroupId(parallel_group_count);
                parallel_group_count += 1;
                for m in members {
                    parallel_id_of_edge.insert(m.index(), gid);
                }
            }
        }

        let mut edge_roles = Vec::with_capacity(n);
        let mut self_loop_count = 0usize;
        for e in edges.iter() {
            let mut set = EdgeRoleSet::default();
            if e.is_self_loop() {
                set.insert(EdgeRole::SelfLoop);
                self_loop_count += 1;
            } else if let Some(gid) = parallel_id_of_edge.get(&e.id.index()) {
                set.insert(EdgeRole::ParallelGroup(*gid));
            } else {
                set.insert(EdgeRole::Forward);
            }
            edge_roles.push(set);
        }

        Self {
            edge_roles,
            port_intents: Vec::new(),
            transit_intents: Vec::new(),
            corridors: Vec::new(),
            side_gutters: Vec::new(),
            merge_intents: Vec::new(),
            circle_membership: vec![None; n],
            topology: RoutingTopologyMetadata {
                edge_count: n,
                self_loop_count,
                parallel_group_count,
            },
            label_policy: LabelPolicy::default(),
        }
    }

    /// 边角色集合（越界返回空引用不便，故返回 Option）。
    pub fn roles_of(&self, id: StableEdgeId) -> Option<&EdgeRoleSet> {
        self.edge_roles.get(id.index())
    }

    /// 供 problem signature 使用的确定性摘要（角色编码序列）。
    ///
    /// 每条边编码为一个稳定 tag，避免依赖任何 HashMap 顺序。
    pub fn role_signature_tokens(&self) -> Vec<String> {
        self.edge_roles
            .iter()
            .map(|set| {
                let parts: Vec<String> = set.as_slice().iter().map(role_tag).collect();
                parts.join("+")
            })
            .collect()
    }
}

fn role_tag(role: &EdgeRole) -> String {
    match role {
        EdgeRole::Forward => "F".to_string(),
        EdgeRole::Feedback => "B".to_string(),
        EdgeRole::SameLayer => "S".to_string(),
        EdgeRole::CrossScope => "X".to_string(),
        EdgeRole::Monitor => "M".to_string(),
        EdgeRole::Business => "Z".to_string(),
        EdgeRole::Pendant => "P".to_string(),
        EdgeRole::SelfLoop => "L".to_string(),
        EdgeRole::ParallelGroup(g) => format!("G{}", g.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{ArrowType, Diagram, Identifier, Relation, SourceInfo, Span, AttributeMap};
    use crate::types::DiagramType;

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

    fn store(relations: Vec<Relation>) -> StableEdgeStore {
        let mut d = Diagram::new(DiagramType::Flowchart, SourceInfo::default());
        d.relations = relations;
        StableEdgeStore::from_diagram(&d)
    }

    #[test]
    fn self_loop_role() {
        let s = store(vec![rel("a", "a")]);
        let c = RoutingContract::compile(&s);
        assert!(c.roles_of(StableEdgeId(0)).unwrap().contains(&EdgeRole::SelfLoop));
        assert_eq!(c.topology.self_loop_count, 1);
    }

    #[test]
    fn parallel_group_role_is_deterministic() {
        // a->b 与 b->a 共享端点 key，应归为同一平行组。
        let s = store(vec![rel("a", "b"), rel("b", "a"), rel("c", "d")]);
        let c = RoutingContract::compile(&s);
        let r0 = c.roles_of(StableEdgeId(0)).unwrap();
        let r1 = c.roles_of(StableEdgeId(1)).unwrap();
        assert!(r0.contains(&EdgeRole::ParallelGroup(ParallelGroupId(0))));
        assert!(r1.contains(&EdgeRole::ParallelGroup(ParallelGroupId(0))));
        // c->d 单边：Forward
        assert!(c.roles_of(StableEdgeId(2)).unwrap().contains(&EdgeRole::Forward));
        assert_eq!(c.topology.parallel_group_count, 1);
    }

    #[test]
    fn role_signature_stable_across_runs() {
        let s = store(vec![rel("a", "b"), rel("b", "a"), rel("x", "x")]);
        let a = RoutingContract::compile(&s).role_signature_tokens();
        let b = RoutingContract::compile(&s).role_signature_tokens();
        assert_eq!(a, b);
        assert_eq!(a, vec!["G0".to_string(), "G0".to_string(), "L".to_string()]);
    }
}
