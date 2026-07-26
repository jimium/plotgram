//! 统一坐标约束求解器 IR 模型。
//!
//! 本模块定义 `CoordinateProblem`——坐标求解的唯一输入。
//! 所有布局语义（结构识别、审美目标、间距需求）在外部编译为 IR，
//! 求解器只消费 IR，不读取 AST 或图类型语义。

// `SolverStatus` 现居中立模块 [`crate::layout::kernel::cost`]（路由/坐标/通道图共用）；
// 此处重导出以保持 `coordinate::model::SolverStatus` 既有路径可用。
pub use crate::layout::kernel::cost::SolverStatus;

// ─── 变量 ─────────────────────────────────────────────────────────────────────

/// 变量 ID：连续索引，热路径用 `Vec` 下标访问。
pub type VarId = usize;

/// 节点变量 kind。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarKind {
    /// 真实节点（有用户语义）。
    Real,
    /// Dummy 节点（长边引入的虚拟节点）。
    Dummy,
    /// 虚拟轴（region axis，不属于任何层）。
    Axis,
}

/// 单个坐标变量。
#[derive(Debug, Clone)]
pub struct NodeVariable {
    /// 连续变量 id（在 `CoordinateProblem.nodes` 中的下标）。
    pub var_id: VarId,
    /// 稳定 id（真实节点为 entity id；dummy 为合成 id；axis 为 region id）。
    pub stable_id: String,
    /// 变量类型。
    pub kind: VarKind,
    /// 所在 rank（axis 变量为 `usize::MAX`）。
    pub rank: usize,
    /// 层内顺序（axis 变量为 `usize::MAX`）。
    pub order: usize,
    /// 主轴方向尺寸（TB 下为 width，LR 下为 height）。
    pub axis_size: f64,
    /// 是否可移动（false 表示 Fixed 硬约束）。
    pub movable: bool,
}

/// 虚拟轴变量（region axis）。
#[derive(Debug, Clone)]
pub struct AxisVariable {
    /// 连续变量 id。
    pub var_id: VarId,
    /// 稳定 id。
    pub stable_id: String,
    /// 所属 region id（预留，首期为 usize）。
    pub region_id: usize,
    /// BK 初值。
    pub initial: f64,
}

// ─── 硬约束 ───────────────────────────────────────────────────────────────────

/// 硬约束来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintSourceKind {
    /// 层内顺序产生的最小分离。
    LayerOrder,
    /// 节点间距。
    NodeSeparation,
    /// 容器/scope 边界。
    ContainerBound,
    /// 用户 pin / fixed。
    UserConstraint,
    /// SpaceBudget 间距需求。
    SpaceBudget,
    /// 路由压力反馈。
    RouteDemand,
}

/// 硬约束来源（带诊断信息）。
#[derive(Debug, Clone)]
pub struct ConstraintSource {
    pub kind: ConstraintSourceKind,
    /// 涉及的节点 stable_id（诊断用）。
    pub nodes: Vec<String>,
    /// 静态说明。
    pub note: &'static str,
}

/// P0 硬约束：定义可行域，任何美学目标不得破坏。
#[derive(Debug, Clone)]
pub enum HardConstraint {
    /// 相邻变量最小分离：`x[right] - x[left] >= distance`。
    MinSeparation {
        left: VarId,
        right: VarId,
        distance: f64,
        source: ConstraintSource,
    },
    /// 变量下界：`x[var] >= value`。
    LowerBound {
        var: VarId,
        value: f64,
        source: ConstraintSource,
    },
    /// 变量上界：`x[var] <= value`。
    UpperBound {
        var: VarId,
        value: f64,
        source: ConstraintSource,
    },
    /// 固定位置：`x[var] == value`。
    Fixed {
        var: VarId,
        value: f64,
        source: ConstraintSource,
    },
    /// 成员须落在组框内（Phase 4.B 槽位；生产求解器首期不投影，仅 IR/shadow）。
    GroupContainment {
        group_index: usize,
        member_var: VarId,
        pad: f64,
        source: ConstraintSource,
    },
    /// 同级组在主/交叉轴上最小分离（Phase 4.B 槽位；首期不投影）。
    GroupSiblingSeparation {
        left_group: usize,
        right_group: usize,
        distance: f64,
        source: ConstraintSource,
    },
}

/// 组在 `CoordinateProblem.groups` 中的下标。
pub type GroupIx = usize;

/// 组角色（来自 LayoutContract；禁止按 DiagramType 分支）。
///
/// 过渡期以 `Container` 为主；将来 `Lane` / `TableCell` 由结构语义 DSL 注入。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GroupRole {
    /// 诚实容器：框 = 成员包围 + padding。
    #[default]
    Container,
}

/// 组框变量（G1）：四边界 + 父子结构 + padding/role。
///
/// 边界 `VarId` 在 `compile_group_variables` 时为 `None`；`attach_group_ir` 按
/// `SolveAxis` 分配当前轴两侧（Cross→left/right，Main→top/bottom）且必须为 `Some`。
/// G3：生产写权经 `LayoutSession::materialize*`；本 IR 供投影与会话物化。
#[derive(Debug, Clone)]
pub struct GroupVariable {
    /// 组稳定 id。
    pub stable_id: String,
    pub left: Option<VarId>,
    pub right: Option<VarId>,
    pub top: Option<VarId>,
    pub bottom: Option<VarId>,
    /// 父组下标（`None` = 顶层）。
    pub parent: Option<GroupIx>,
    /// 直接子组下标（按 stable_id 排序，确定性）。
    pub children: Vec<GroupIx>,
    /// 成员节点 stable_id（构建时按 id 排序，保证确定性）。
    pub members: Vec<String>,
    /// 容纳约束参数（非事后加数）。
    pub padding: crate::layout::kernel::group::bounds::GroupPadding,
    /// 组角色。
    pub role: GroupRole,
}

// ─── 目标 ─────────────────────────────────────────────────────────────────────

/// 目标优先级层级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObjectivePriority {
    /// P1：高置信结构目标（split-join 共轴、fan 对称、chain 共轴）。
    P1 = 1,
    /// P2：边与路由友好目标（短边、dummy 共线、spacing demand）。
    P2 = 2,
    /// P3：普通审美目标（BK 稳定性、紧凑度、移动量最小）。
    P3 = 3,
}

/// 统一二次目标项：`loss = weight * (constant + Σ coefficients[i] * x[var])²`。
///
/// 可表达：共轴 `(x[u]-x[v])²`、质心对齐、镜像、等间距、靠近初值等。
#[derive(Debug, Clone)]
pub struct ObjectiveTerm {
    /// 优先级层级。
    pub priority: ObjectivePriority,
    /// 稀疏线性系数：`(var_id, coefficient)`。
    pub coefficients: Vec<(VarId, f64)>,
    /// 常数项。
    pub constant: f64,
    /// 权重（同 priority 内的相对权重）。
    pub weight: f64,
    /// 来源诊断。
    pub source: ConstraintSource,
}

// ─── 层约束集 ─────────────────────────────────────────────────────────────────

/// 一层的变量序列和层内硬约束。
///
/// 变量顺序在构建时冻结，`MinSeparation` 直接编码该顺序。
#[derive(Debug, Clone)]
pub struct LayerConstraintSet {
    /// rank 索引。
    pub rank: usize,
    /// 层内变量 id（按层内顺序排列）。
    pub vars: Vec<VarId>,
    /// 相邻变量间的最小分离距离（长度 = vars.len() - 1）。
    pub separations: Vec<f64>,
}

// ─── 初值 ─────────────────────────────────────────────────────────────────────

/// BK 生成的初始坐标。
#[derive(Debug, Clone)]
pub struct InitialCoordinates {
    /// 每个变量的初始主轴坐标（下标 = var_id）。
    pub values: Vec<f64>,
}

// ─── 求解器配置 ───────────────────────────────────────────────────────────────

/// 求解器配置。
#[derive(Debug, Clone)]
pub struct CoordinateSolverConfig {
    /// P1 最大迭代次数。
    pub max_iter_p1: usize,
    /// P2 最大迭代次数。
    pub max_iter_p2: usize,
    /// P3 最大迭代次数。
    pub max_iter_p3: usize,
    /// 收敛 epsilon（坐标变化量）。
    pub epsilon: f64,
    /// 初始步长。
    pub initial_step: f64,
    /// 步长 backoff 最大次数。
    pub max_step_backoff: usize,
    /// P1 loss 容差（P2/P3 优化时允许 P1 增加的上限）。
    pub p1_tolerance: f64,
}

impl Default for CoordinateSolverConfig {
    fn default() -> Self {
        Self {
            max_iter_p1: 80,
            max_iter_p2: 40,
            max_iter_p3: 30,
            epsilon: 0.01,
            initial_step: 1.0,
            max_step_backoff: 8,
            p1_tolerance: 0.0, // 首期由外部按 node_gap 设定
        }
    }
}

// ─── 求解轴 ───────────────────────────────────────────────────────────────────

/// 求解轴标注：告知 solver/materializer 当前求解的是哪个物理轴。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SolveAxis {
    /// 横轴（TB 布局下的 x，LR 布局下的 y）。
    #[default]
    Cross,
    /// 主轴（TB 布局下的 y，LR 布局下的 x）。
    Main,
}

// ─── 顶层问题对象 ─────────────────────────────────────────────────────────────

/// 坐标求解问题的完整 IR。
///
/// 由外部 builder 构建，solver 只消费此结构。
#[derive(Debug, Clone)]
pub struct CoordinateProblem {
    /// 所有变量（节点 + dummy + axis），下标即 VarId。
    pub vars: Vec<NodeVariable>,
    /// 每层的约束集（按 rank 排序）。
    pub layers: Vec<LayerConstraintSet>,
    /// P0 硬约束（层内分离已编码在 layers 中，此处放跨层/边界/固定约束）。
    pub hard: Vec<HardConstraint>,
    /// 软目标（按 priority 分组使用）。
    pub objectives: Vec<ObjectiveTerm>,
    /// BK 初值。
    pub initial: InitialCoordinates,
    /// 求解器配置。
    pub config: CoordinateSolverConfig,
    /// 求解轴标注（告知 materializer 坐标写回哪个物理轴）。
    pub axis: SolveAxis,
    /// 组框变量（Phase 4；默认可空）。
    pub groups: Vec<GroupVariable>,
}

impl CoordinateProblem {
    /// G5：统一生产构造门面（flat / arch / intra 经此组装；mindmap 仍白名单债）。
    pub fn build(
        vars: Vec<NodeVariable>,
        layers: Vec<LayerConstraintSet>,
        hard: Vec<HardConstraint>,
        objectives: Vec<ObjectiveTerm>,
        initial: InitialCoordinates,
        axis: SolveAxis,
    ) -> Self {
        Self {
            vars,
            layers,
            hard,
            objectives,
            initial,
            config: CoordinateSolverConfig::default(),
            axis,
            groups: Vec::new(),
        }
    }

    /// 变量数量。
    pub fn var_count(&self) -> usize {
        self.vars.len()
    }

    /// 按 priority 过滤 objectives。
    pub fn objectives_by_priority(&self, p: ObjectivePriority) -> impl Iterator<Item = &ObjectiveTerm> {
        self.objectives.iter().filter(move |t| t.priority == p)
    }
}

// ─── 求解结果 ─────────────────────────────────────────────────────────────────

/// 求解结果。
#[derive(Debug, Clone)]
pub struct SolverResult {
    /// 最终坐标（下标 = var_id）。
    pub coordinates: Vec<f64>,
    /// 状态。
    pub status: SolverStatus,
    /// 各 priority 的最终 loss。
    pub loss_p1: f64,
    pub loss_p2: f64,
    pub loss_p3: f64,
    /// 总迭代次数。
    pub iterations: usize,
    /// 硬约束最大违反量（0 = 无违反）。
    pub max_hard_violation: f64,
    /// 结构化诊断信息。
    pub diagnostics: SolverDiagnostics,
}

/// 结构化求解诊断（替代旧版 `Vec<String>`）。
#[derive(Debug, Clone, Default)]
pub struct SolverDiagnostics {
    /// 投影轮次（Dykstra 交替投影实际执行轮数）。
    pub projection_rounds: usize,
    /// 各 phase 实际迭代次数 [P1, P2, P3]。
    pub phase_iterations: [usize; 3],
    /// 活跃分离约束数（被 PAVA 触碰的 separation 数）。
    pub active_separation_count: usize,
    /// 连通分量数（objective/hard 关联图的 weak components）。
    pub component_count: usize,
    /// 文本诊断（兼容旧日志）。
    pub notes: Vec<String>,
}

impl SolverResult {
    /// 是否可行（硬约束全部满足）。
    pub fn is_feasible(&self) -> bool {
        self.status != SolverStatus::Infeasible
    }

    /// 安全获取坐标：仅在可行时返回 Some。
    ///
    /// 调用方应优先使用此方法，避免物化不可行解。
    pub fn feasible_coordinates(&self) -> Option<&[f64]> {
        if self.is_feasible() {
            Some(&self.coordinates)
        } else {
            None
        }
    }
}

// ─── 诊断 ─────────────────────────────────────────────────────────────────────

