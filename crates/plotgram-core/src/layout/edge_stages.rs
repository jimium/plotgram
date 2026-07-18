//! A3：边路由后处理管线的**声明式阶段写权表**。
//!
//! 本模块不参与调度、不改变任何执行顺序与结果——它是「管线时序」的
//! 可执行文档：把 [`crate::layout::pipeline::LayoutPipeline::run_routing_pipeline`]
//! 中 18 个后处理步骤各自「写什么」（节点 / 折点 / label / annotation / 分组 /
//! 画布）显式登记下来，可通过 `PLOTGRAM_DUMP_EDGE_STAGES` 环境变量转储审计。
//!
//! 配套的两个几何冻结屏障（节点冻结 / 折线冻结）见 [`NodeFreeze`] 与
//! [`PolylineFreeze`]（A3 双冻结点）。
//!
//! 设计约束（AGENTS.md §5 + 重构手册）：
//! - **顺序与效果不变**：本表是纯描述，不驱动迭代；dump 靠 env 门控，默认零输出。
//! - **不重排阶段**、**不引入空壳 EdgeGeometryContract**。

/// 后处理阶段的写入目标：登记「这一步会改动哪些几何/语义状态」。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteTarget {
    /// 节点坐标 (x, y)——节点冻结屏障之后不得再出现。
    Nodes,
    /// 边折点 / 路径几何——折线冻结屏障之后不得再出现。
    Points,
    /// 边 label 位置。
    Labels,
    /// route_annotation 语义。
    Annotations,
    /// 分组包围框。
    Groups,
    /// 画布边界。
    CanvasBounds,
}

/// 单个后处理阶段的声明式写权条目。
#[derive(Debug, Clone, Copy)]
pub struct EdgeStage {
    /// 管线内 1-based 序号（与 `run_routing_pipeline` 的实际执行顺序一致）。
    pub index: u8,
    /// 阶段函数名 / 语义标识。
    pub name: &'static str,
    /// 该阶段会写入的状态目标。
    pub writes: &'static [WriteTarget],
    /// 备注：适用条件（如 `[arch]`）、冻结屏障标注等。
    pub note: &'static str,
}

/// 边路由后处理管线的 18 步写权表。
///
/// 顺序即 `run_routing_pipeline` 的实际执行顺序；步骤 12-18 仅在
/// `edge_routing_style == "orthogonal"` 时执行。此表随管线改动手动维护。
pub const POST_ROUTE_STAGES: &[EdgeStage] = &[
    EdgeStage {
        index: 1,
        name: "complete_routing",
        writes: &[WriteTarget::Points, WriteTarget::Labels],
        note: "首轮路由产出边几何与初始 label",
    },
    EdgeStage {
        index: 2,
        name: "repulse_edges_only",
        writes: &[WriteTarget::Points],
        note: "路由后仅几何排斥（不含量化）",
    },
    EdgeStage {
        index: 3,
        name: "run_post_route_group_frame",
        writes: &[
            WriteTarget::Groups,
            WriteTarget::Nodes,
            WriteTarget::Points,
            WriteTarget::CanvasBounds,
        ],
        note: "组框 restore/重路由，可能挪节点",
    },
    EdgeStage {
        index: 4,
        name: "hook.after_route",
        writes: &[WriteTarget::Points, WriteTarget::Nodes, WriteTarget::Groups],
        note: "算法专属后处理钩子（arch 可挪节点）",
    },
    EdgeStage {
        index: 5,
        name: "resolve_budget_violations",
        writes: &[WriteTarget::Nodes],
        note: "SpaceBudget 兜底移节点",
    },
    EdgeStage {
        index: 6,
        name: "group_frame restore/recompute",
        writes: &[WriteTarget::Groups, WriteTarget::Nodes],
        note: "budget guard 后恢复 L1 契约（track:equal 走 restore）",
    },
    EdgeStage {
        index: 7,
        name: "reassert_multi_client_hub_centroids",
        writes: &[WriteTarget::Nodes],
        note: "[arch] 局部刚体重申 hub 质心",
    },
    EdgeStage {
        index: 8,
        name: "align_cross_scope_pendant_chains",
        writes: &[WriteTarget::Nodes],
        note: "[arch] 跨 scope 悬挂链对齐（explicit_equal）",
    },
    EdgeStage {
        index: 9,
        name: "record_moved_for_overlap",
        writes: &[],
        note: "记账：收集本轮被挪节点，不写几何",
    },
    EdgeStage {
        index: 10,
        name: "reroute_and_repulse",
        writes: &[WriteTarget::Points],
        note: "对被挪节点重路由 —— 节点冻结屏障（此后禁止再挪节点）",
    },
    EdgeStage {
        index: 11,
        name: "snap_and_repulse_edges_with_guard",
        writes: &[WriteTarget::Points],
        note: "像素量化，管道末尾仅运行一次",
    },
    EdgeStage {
        index: 12,
        name: "sanitize_orthogonal_edges_with_guard",
        writes: &[WriteTarget::Points, WriteTarget::Labels],
        note: "[orthogonal] 量化后消毒，overshoot Z 合并；按平行边规则重建 label",
    },
    EdgeStage {
        index: 13,
        name: "enforce_reverse_pair_min_gap",
        writes: &[WriteTarget::Points],
        note: "[orthogonal] D 正反向 gap 审计",
    },
    EdgeStage {
        index: 14,
        name: "enforce_reverse_pair_dock_separation",
        writes: &[WriteTarget::Points],
        note: "[orthogonal] 正反向同侧 dock 共锚（D 末最终写者）",
    },
    EdgeStage {
        index: 15,
        name: "resolve_exact_stub_occupancy_post_route",
        writes: &[WriteTarget::Points, WriteTarget::Annotations],
        note: "[arch] 节点冻结后 exact 跨对共柱真修，刷新 annotation",
    },
    EdgeStage {
        index: 16,
        name: "repair_through_edges_post_route",
        writes: &[WriteTarget::Points],
        note: "[orthogonal] 保组 dogleg 试修 —— 折线冻结屏障（此后仅允许 label/annotation）",
    },
    EdgeStage {
        index: 17,
        name: "resolve_label_overlaps_with_config",
        writes: &[WriteTarget::Labels],
        note: "[orthogonal] 几何冻结后 label 避让（最终步骤）",
    },
    EdgeStage {
        index: 18,
        name: "dedupe_labels_on_declared_merges",
        writes: &[WriteTarget::Labels],
        note: "[orthogonal] 声明合并边的 label 去重",
    },
];

/// 转储后处理写权表（`PLOTGRAM_DUMP_EDGE_STAGES` 置位时生效，默认零输出）。
pub fn dump_edge_stages() {
    if std::env::var_os("PLOTGRAM_DUMP_EDGE_STAGES").is_none() {
        return;
    }
    crate::perf_log!("[edge-stages] 后处理写权表（顺序即执行顺序，仅供审计）:");
    for st in POST_ROUTE_STAGES {
        crate::perf_log!(
            "  {:>2}. {:<38} writes={:?} — {}",
            st.index,
            st.name,
            st.writes,
            st.note
        );
    }
}

/// **节点冻结屏障**（A3）：快照节点指纹，之后断言未变。
///
/// 用于 step 10 `reroute_and_repulse` 之后：此后所有阶段仅应改边几何 / label /
/// annotation，**不得再挪节点**。以 `debug_assert!` 钉死——release（含
/// `cargo test --release`）会编译掉，对 fp / 性能零影响，完全符合「顺序效果不变」。
pub struct NodeFreeze {
    #[cfg(debug_assertions)]
    fingerprint: String,
}

impl NodeFreeze {
    /// 在节点冻结点采集指纹（复用 metrics 的 `node_fingerprint` 约定）。
    /// release 构建下不计算指纹，与 `debug_assert` 一同被编译掉。
    pub fn capture(result: &crate::layout::LayoutResult) -> Self {
        #[cfg(not(debug_assertions))]
        let _ = result;
        Self {
            #[cfg(debug_assertions)]
            fingerprint: crate::layout::metrics::node_fingerprint(result),
        }
    }

    /// 断言节点未变；debug 构建下若被挪动即 panic（定位破坏冻结契约的上游阶段）。
    pub fn assert_unchanged(&self, result: &crate::layout::LayoutResult) {
        #[cfg(not(debug_assertions))]
        let _ = result;
        #[cfg(debug_assertions)]
        debug_assert_eq!(
            self.fingerprint,
            crate::layout::metrics::node_fingerprint(result),
            "A3 节点冻结屏障被破坏：step 10 之后仍有阶段挪动了节点（应只改边几何/label）"
        );
    }
}

/// **折线冻结屏障**（A3）：快照边折线指纹，之后软校验。
///
/// 用于 step 16 `repair_through_edges_post_route` 之后：此后（label 避让 / 去重）
/// 只允许改 label 与 annotation。折点若仍变动则打 warning、视为上游 bug——采用
/// 软校验而非 `debug_assert`，因为 doc 意图容忍幂等微调，硬断言过严。
pub struct PolylineFreeze {
    fingerprint: u64,
}

impl PolylineFreeze {
    /// 在折线冻结点采集指纹。
    pub fn capture(result: &crate::layout::LayoutResult) -> Self {
        Self {
            fingerprint: polyline_fingerprint(result),
        }
    }

    /// 若折点在冻结后仍变动，打 warning（不 panic）。
    pub fn warn_if_changed(&self, result: &crate::layout::LayoutResult) {
        if self.fingerprint != polyline_fingerprint(result) {
            crate::perf_log!(
                "[warn] A3 折线冻结屏障：step 16 之后 label 阶段改动了边折点（应只改 label/annotation），疑似上游 bug"
            );
        }
    }
}

/// 边折线指纹：按边序（稳定）遍历 anchor 折点，量化后 fnv1a64 哈希。
fn polyline_fingerprint(result: &crate::layout::LayoutResult) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |v: i64| {
        for b in v.to_le_bytes() {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    };
    for edge in &result.edges {
        for p in edge.geometry.anchor_points().iter() {
            mix((p.x * 100.0).round() as i64);
            mix((p.y * 100.0).round() as i64);
        }
        // 边界分隔符：避免相邻边折点拼接歧义。
        mix(i64::MIN);
    }
    hash
}
