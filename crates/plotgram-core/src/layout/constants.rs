//! 布局常量集中定义
//!
//! 将各布局算法与边路由中重复使用的默认常量统一收归于此，
//! 便于维护与全局调整。各模块可按需 `use` 引入。

// ─── 节点尺寸 ──────────────────────────────────────────

/// 默认节点宽度
pub const DEFAULT_NODE_WIDTH: f64 = 160.0;

/// 默认节点高度
pub const DEFAULT_NODE_HEIGHT: f64 = 50.0;

// ─── 间距 ──────────────────────────────────────────────

/// 默认画布内边距（所有布局统一使用）
pub const DEFAULT_PADDING: f64 = 70.0;

/// 默认分组内边距（force_directed / sequence / circular；sugiyama 系列使用 28.0）
pub const DEFAULT_GROUP_PADDING: f64 = 20.0;

/// 默认节点 margin（不可见外边框）：边路由时节点障碍物膨胀间距。
/// 取 orthogonal 路由的 `NODE_OBSTACLE_PAD` 值，保持默认行为不变。
pub const DEFAULT_NODE_MARGIN: f64 = 18.0;

/// 默认分组 margin（不可见外边框）：边路由通道绕行时距分组边框的留白。
pub const DEFAULT_GROUP_MARGIN: f64 = 18.0;

// ─── 边路由 ────────────────────────────────────────────

/// 默认边偏移量（边与节点的最小间距）
pub const DEFAULT_EDGE_OFFSET: f64 = 24.0;

/// 默认障碍物内边距（可见性图与样条路由中使用）
pub const DEFAULT_OBSTACLE_PADDING: f64 = 8.0;

/// 默认最小分离距离
pub const DEFAULT_MIN_SEPARATION: f64 = 2.0;

/// orthogonal 路由 slot 磁吸点间距（orthogonal 路由与 friendliness/port_conflict 共享）
pub const ORTHO_SLOT_PITCH: f64 = 40.0;

/// orthogonal 路由平行边重叠判定阈值（orthogonal scoring 与 refine/segments_conflict_xy 共享）
pub const ORTHO_PARALLEL_GAP: f64 = 8.0;

// ─── 标签 ──────────────────────────────────────────────

/// 默认标签垂直偏移（标签相对边路径的垂直距离）
pub const DEFAULT_LABEL_PERP_OFFSET: f64 = 8.0;

/// 默认标签字号
pub const DEFAULT_LABEL_FONT_SIZE: f64 = 11.0;

/// 默认标签内边距（circular 使用 6.0）
pub const DEFAULT_LABEL_PADDING: f64 = 4.0;

/// 默认 CJK 字符宽度
pub const DEFAULT_CJK_CHAR_WIDTH: f64 = 11.0;

/// 默认 ASCII 字符宽度
pub const DEFAULT_ASCII_CHAR_WIDTH: f64 = 6.5;

/// 默认标签位置迭代次数上限
pub const DEFAULT_MAX_LABEL_ITERATIONS: usize = 5;

/// 引线触发阈值：标签中心到边路径距离超过此值时绘制引线
pub const DEFAULT_LEADER_LINE_THRESHOLD: f64 = 4.0;

// ─── 算法专用常量 ──────────────────────────────────────

/// Sugiyama 算法分组内边距（比默认值大，为分层布局留出更多空间）
pub const SUGIYAMA_GROUP_PADDING: f64 = 28.0;

/// 圆形布局：画布内边距、多分量间距
pub const CIRCULAR_PADDING: f64 = 70.0;
pub const CIRCULAR_COMPONENT_GAP: f64 = 40.0;

/// 径向/力导向布局画布内边距
pub const WIDE_PADDING: f64 = 70.0;

/// Force-directed-fr 默认节点宽度（比标准值略窄，适配散布布局）
pub const FR_NODE_WIDTH: f64 = 156.0;

/// 思维导图布局默认间距
pub const MINDMAP_PADDING: f64 = WIDE_PADDING;
pub const MINDMAP_LEVEL_GAP: f64 = 200.0;
pub const MINDMAP_BRANCH_GAP: f64 = 70.0;
pub const MINDMAP_NODE_GAP: f64 = 22.0;
pub const MINDMAP_CENTER_GAP: f64 = 100.0;

/// 时序图布局默认间距
pub const SEQUENCE_NODE_SPACING: f64 = 80.0;
pub const SEQUENCE_MESSAGE_SPACING: f64 = 50.0;

/// 力导向布局默认间距（`group_padding` 默认与 `GroupPadding::force_directed` 水平边距一致）
pub const FORCE_DIRECTED_COMPONENT_GAP: f64 = 120.0;
pub const FORCE_DIRECTED_GROUP_PADDING: f64 = 20.0;

/// architecture 画布与分组内边距
pub const ARCH_V2_PADDING: f64 = DEFAULT_PADDING;
pub const ARCH_V2_GROUP_PADDING: f64 = SUGIYAMA_GROUP_PADDING;

// ─── 网格吸附（Grid Snap）────────────────────────────────

/// 像素网格步长（组框、边拐点等，Phase 2 使用）
pub const GRID_SNAP_STEP: f64 = 8.0;

/// 层聚类容差：rank 轴中心距小于此值的节点视为同层
pub const GRID_SNAP_LAYER_TOLERANCE: f64 = 4.0;

/// 单节点 snap 最大允许位移，超过则跳过
pub const GRID_SNAP_MAX_DISTANCE: f64 = 24.0;

/// architecture 槽位间距
pub const GRID_SNAP_NODE_GAP_ARCH: f64 = 48.0;

/// sugiyama-v2 槽位间距
pub const GRID_SNAP_NODE_GAP_SUGIYAMA: f64 = 56.0;
