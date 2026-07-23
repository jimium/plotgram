//! 统一坐标约束求解器 IR 模型。
//!
//! 本模块定义 `CoordinateProblem`——坐标求解的唯一输入。
//! 所有布局语义（结构识别、审美目标、间距需求）在外部编译为 IR，
//! 求解器只消费 IR，不读取 AST 或图类型语义。

use std::fmt;

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
}

impl CoordinateProblem {
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

/// 求解结果状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolverStatus {
    /// 正常收敛。
    Converged,
    /// 未收敛但返回最佳可行解。
    Degraded,
    /// 硬约束不可行。
    Infeasible,
}

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
    /// 诊断信息。
    pub diagnostics: Vec<String>,
}

// ─── 诊断 ─────────────────────────────────────────────────────────────────────

impl fmt::Display for SolverStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Converged => write!(f, "converged"),
            Self::Degraded => write!(f, "degraded"),
            Self::Infeasible => write!(f, "infeasible"),
        }
    }
}
