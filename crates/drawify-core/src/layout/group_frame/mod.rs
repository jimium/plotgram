//! Group Frame — 组间宏观几何统一层（L1）。
//!
//! 本模块定义组间「排列 + 尺寸 + 对齐 + 间距 + 量化」的统一规格 [`GroupFrameSpec`]，
//! 并提供从 diagram 属性解析 Spec 的 [`resolve_group_frame_spec`]。
//!
//! ## 三层 Frame 模型
//!
//! ```text
//! ┌──────────────────────────────────────────────────────────┐
//! │  L1  Group Frame（组间）   — 顶层/同级 group 的 track 几何  │  ← 本模块
//! ├──────────────────────────────────────────────────────────┤
//! │  L2  Intra Frame（组内）   — 单 group 内节点的排列模式      │  group_layout_hint
//! ├──────────────────────────────────────────────────────────┤
//! │  L3  Node Frame（节点）   — rank/layer 对齐 + 像素量化     │  grid_snap
//! └──────────────────────────────────────────────────────────┘
//! ```
//!
//! P0 阶段仅提供类型与解析，不调用布局算法。`apply_group_frame` 整形 Pass 见 P1。
//!
//! 详见 `docs/architecture/布局优化/group-frame-spec.md`（v0.2）。

use crate::ast::{AttributeValue, Diagram};
use crate::layout::grid_snap::{diagram_snap_attribute, should_snap, snap_floor, snap_ceil};
use crate::layout::intent::PinSet;
use crate::layout::node::common::group_bounds::{compute_group_bounds, GroupPadding as BoundsGroupPadding};
use crate::layout::{GroupLayout, LayoutResult, NodeLayout};
use crate::types::standard_attr_keys::diagram as dsl;
use std::collections::{HashMap, HashSet};

mod realign;
mod pass;

pub use pass::{group_padding_from_plan, GroupFramePass};
pub use realign::realign_group_rows;

// ─── 基础类型 ─────────────────────────────────────────────

/// 一维坐标轴。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Axis {
    /// 水平轴（x 方向）
    Horizontal,
    /// 垂直轴（y 方向）
    Vertical,
}

// ─── Spec 结构 ────────────────────────────────────────────

/// 组间 Frame 规格（从 DSL / 默认值解析）。
///
/// 详见 spec §3.1。
#[derive(Debug, Clone)]
pub struct GroupFrameSpec {
    /// 主排列：一维 stack 或（二期）二维 matrix
    pub arrangement: GroupArrangement,
    /// 主方向 track 的尺寸策略
    pub track_sizing: TrackSizing,
    /// 交叉轴对齐。
    ///
    /// **语义耦合**：仅在 [`TrackSizing::Fit`] 下有意义；[`TrackSizing::Equal`] 时
    /// 所有 track 等宽，cross 轴几何已由 sizing 决定，`cross_align` 退化为
    /// 「组内内容在 track 内的对齐」。
    pub cross_align: CrossAlign,
    /// 组间净间距（gutter）
    pub gap: f64,
    /// 组内 padding：Frame 消费 `compute_group_bounds` 产出的 `GroupLayout.width`，
    /// 该 width 已含 padding。Frame 不直接施加 padding，但 Equal 的 `max(content)`
    /// 隐含 padding 参与。此处保留字段供二期 `Fixed` sizing 与报告诊断使用。
    pub padding: GroupPadding,
    /// 边框共线策略
    pub border_align: BorderAlign,
    /// 像素量化（可与 L3 合并配置）
    pub quantize: QuantizeSpec,
}

/// 组间排列方式。
#[derive(Debug, Clone, PartialEq)]
pub enum GroupArrangement {
    /// 一维堆叠（流程图阶段图 / 泳道 / 架构顶层条带）
    Stack { axis: Axis },
    /// 二期：显式行列（多行多列 group 矩阵）。
    ///
    /// 命名用 `Matrix` 而非 `Grid`，避免与 L2 `GroupLayoutHint::Grid`（组内矩阵）
    /// 及 L3 `grid_snap`（节点 8px snap）三层「grid」歧义（见 spec §1.3）。
    Matrix { rows: Option<u32>, cols: Option<u32> },
}

/// 主方向 track 的尺寸策略。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TrackSizing {
    /// 每 track 贴合内容（默认）
    Fit,
    /// 同级 track 等宽/等高（现 `group_sizing: uniform`）
    Equal,
    /// 固定尺寸（二期）
    Fixed(f64),
}

/// 交叉轴对齐。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossAlign {
    /// 起/左/顶对齐（现 flowchart `group_align: left` / architecture 左缘对齐）
    Start,
    /// 居中（现 `group_align: center`）
    Center,
    /// 末/右/底对齐
    End,
    /// 拉满 cross 轴（二期）
    Stretch,
}

/// 边框共线策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderAlign {
    /// 不做边框共线
    None,
    /// 同级 group 同侧边框共线（左/顶优先）
    SharedLines,
}

/// 像素量化规格。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuantizeSpec {
    pub enabled: bool,
    /// 吸附步长（默认 8.0）
    pub step: f64,
    /// 是否量化 group 边框
    pub quantize_groups: bool,
    /// 是否量化节点（L3 是否由同一配置驱动）
    pub quantize_nodes: bool,
}

impl Default for QuantizeSpec {
    fn default() -> Self {
        Self {
            enabled: true,
            step: 8.0,
            quantize_groups: true,
            quantize_nodes: true,
        }
    }
}

/// 组内 padding（与 `node::common::group_bounds::GroupPadding` 对齐）。
///
/// 非对称：architecture 用 (x=28, y_top=48, x_delta=56, y_delta=76)；
/// flowchart/sugiyama 用 `uniform(group_padding, header_height)`。
/// Frame 不直接施加 padding，而是消费 `compute_group_bounds` 的结果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupPadding {
    pub x: f64,
    pub y_top: f64,
    pub x_delta: f64,
    pub y_delta: f64,
}

impl GroupPadding {
    /// architecture_v2 默认 padding（与 `GroupPaddingLike::architecture_v2` 一致）
    pub fn architecture_v2() -> Self {
        Self {
            x: 28.0,
            y_top: 48.0,
            x_delta: 56.0,
            y_delta: 76.0,
        }
    }

    /// 从对称 padding + header 高度构造（sugiyama / flowchart 路径）
    pub fn uniform(padding: f64, header_height: f64) -> Self {
        let p = crate::layout::node::common::group_bounds::GroupPadding::uniform(
            padding,
            header_height,
        );
        Self {
            x: p.x,
            y_top: p.y_top,
            x_delta: p.x_delta,
            y_delta: p.y_delta,
        }
    }
}

// ─── 解析 ─────────────────────────────────────────────────

/// 默认量化步长。
const DEFAULT_QUANTIZE_STEP: f64 = 8.0;

/// group 标签高度（与 `architecture_v2::layout::GROUP_LABEL_HEIGHT` 一致）。
const GROUP_LABEL_HEIGHT: f64 = 20.0;

/// architecture 顶层 group 水平间距（与 `architecture_v2::layout::GROUP_GAP_X` 一致）。
const ARCH_GROUP_GAP: f64 = 50.0;

/// 从 diagram 属性 + 算法名解析 [`GroupFrameSpec`]。
///
/// # 算法默认值
///
/// | 算法 | arrangement | track_sizing | cross_align | gap | border_align |
/// |------|-------------|--------------|-------------|-----|--------------|
/// | `architecture` | `Stack(H)` | `Fit`（`uniform` 时 `Equal`） | `Start` | 50.0 | `SharedLines` |
/// | `flowchart` | `Stack(V)`（可由 `group_arrangement` 覆盖） | `Fit` | `Center`（可由 `group_align` 覆盖） | 60.0（可由 `group_gap` 覆盖） | `None` |
/// | 其他含 group 算法 | `Stack(V)` | `Fit` | `Center` | 60.0 | `None` |
///
/// `quantize.enabled` 由 `snap` 属性 + 算法白名单（[`should_snap`]）决定。
///
/// # `group_frame:` 配置块（spec §6.2 sugar）
///
/// 若 diagram 声明了 `group_frame: stack { axis: horizontal, gap: 48, ... }` 配置块，
/// 则以配置块覆盖算法默认值；未声明的字段保留算法默认。旧属性（`group_sizing` 等）
/// 保留为 sugar，在无 `group_frame` 配置块时生效。
pub fn resolve_group_frame_spec(diagram: &Diagram, algo: &str) -> GroupFrameSpec {
    // 优先消费 `group_frame:` 配置块（覆盖算法默认值）
    if let Some(spec) = resolve_from_group_frame_config(diagram, algo) {
        return spec;
    }
    if algo == "architecture" {
        resolve_architecture(diagram, algo)
    } else {
        // flowchart / er / sugiyama-v2 等含 group 的算法走通用 stack 解析
        resolve_stack(diagram, algo)
    }
}

/// 从 `group_frame: stack { ... }` 配置块解析 Spec，覆盖算法默认值。
///
/// 配置块选项：
/// - `axis`: `"horizontal"` | `"vertical"`（stack 排列轴）
/// - `gap`: number（组间间距）
/// - `track`: `"fit"` | `"equal"` | `"uniform"`（track 尺寸策略）
/// - `cross`: `"start"` | `"center"` | `"end"` | `"stretch"`（交叉轴对齐）
/// - `border`: `"none"` | `"shared"` | `"shared_lines"`（边框共线策略）
/// - `snap`: number（量化步长）或 boolean（开关）
///
/// 返回 `None` 表示未声明 `group_frame` 配置块。
fn resolve_from_group_frame_config(diagram: &Diagram, algo: &str) -> Option<GroupFrameSpec> {
    let attr = diagram
        .attributes
        .iter()
        .find(|a| a.key == dsl::GROUP_FRAME)?;
    let (arrangement_algo, options) = match &attr.value {
        AttributeValue::Config { algo, options } => (algo.as_str(), options),
        // `group_frame: stack`（无选项块）→ 仅指定 arrangement，其余用算法默认
        AttributeValue::String(s) => (s.as_str(), &HashMap::new()),
        _ => return None,
    };

    // 以算法默认 Spec 为基底，逐字段覆盖
    let mut spec = if algo == "architecture" {
        resolve_architecture(diagram, algo)
    } else {
        resolve_stack(diagram, algo)
    };

    // arrangement（algo 字段）
    match arrangement_algo {
        "stack" => {
            // axis 可由 options 覆盖
            if let Some(axis) = read_str_option(options, "axis") {
                let axis = match axis.to_ascii_lowercase().as_str() {
                    "horizontal" | "h" => Axis::Horizontal,
                    _ => Axis::Vertical,
                };
                spec.arrangement = GroupArrangement::Stack { axis };
            }
        }
        "matrix" => {
            let rows = read_num_option(options, "rows").map(|n| n as u32);
            let cols = read_num_option(options, "cols").map(|n| n as u32);
            spec.arrangement = GroupArrangement::Matrix { rows, cols };
        }
        _ => {}
    }

    // gap
    if let Some(g) = read_num_option(options, "gap") {
        if g > 0.0 {
            spec.gap = g;
        }
    }

    // track（track_sizing）
    if let Some(t) = read_str_option(options, "track") {
        spec.track_sizing = match t.to_ascii_lowercase().as_str() {
            "fit" => TrackSizing::Fit,
            "equal" | "uniform" => TrackSizing::Equal,
            other => {
                if let Ok(n) = other.parse::<f64>() {
                    TrackSizing::Fixed(n)
                } else {
                    spec.track_sizing
                }
            }
        };
    }

    // cross（cross_align）
    if let Some(c) = read_str_option(options, "cross") {
        spec.cross_align = match c.to_ascii_lowercase().as_str() {
            "start" | "left" => CrossAlign::Start,
            "center" => CrossAlign::Center,
            "end" | "right" => CrossAlign::End,
            "stretch" => CrossAlign::Stretch,
            _ => spec.cross_align,
        };
    }

    // border（border_align）
    if let Some(b) = read_str_option(options, "border") {
        spec.border_align = match b.to_ascii_lowercase().as_str() {
            "none" => BorderAlign::None,
            "shared" | "shared_lines" => BorderAlign::SharedLines,
            _ => spec.border_align,
        };
    }

    // snap（quantize）
    if let Some(snap_val) = options.get("snap") {
        match snap_val {
            AttributeValue::Boolean(enabled) => {
                spec.quantize.enabled = *enabled;
                spec.quantize.quantize_groups = *enabled;
                spec.quantize.quantize_nodes = *enabled;
            }
            AttributeValue::Number(step) => {
                spec.quantize.enabled = true;
                spec.quantize.step = *step;
                spec.quantize.quantize_groups = true;
                spec.quantize.quantize_nodes = true;
            }
            _ => {}
        }
    }

    Some(spec)
}

/// 从 Config options 读取字符串值（小写归一化）。
fn read_str_option<'a>(
    options: &'a HashMap<String, AttributeValue>,
    key: &str,
) -> Option<&'a str> {
    options.get(key).and_then(|v| v.as_str())
}

/// 从 Config options 读取数值。
fn read_num_option(options: &HashMap<String, AttributeValue>, key: &str) -> Option<f64> {
    match options.get(key)? {
        AttributeValue::Number(n) => Some(*n),
        _ => None,
    }
}

/// architecture 默认：`Stack(H) + Fit + Start + gap=50 + SharedLines`；
/// `group_sizing: uniform` 时 `track_sizing = Equal`。
fn resolve_architecture(diagram: &Diagram, algo: &str) -> GroupFrameSpec {
    let track_sizing = if diagram_group_sizing_is_uniform(diagram) {
        TrackSizing::Equal
    } else {
        TrackSizing::Fit
    };

    GroupFrameSpec {
        arrangement: GroupArrangement::Stack {
            axis: Axis::Horizontal,
        },
        track_sizing,
        // architecture 当前 `align_top_groups_horizontally` 无条件左缘对齐
        cross_align: CrossAlign::Start,
        gap: ARCH_GROUP_GAP,
        padding: GroupPadding::architecture_v2(),
        border_align: BorderAlign::SharedLines,
        quantize: resolve_quantize(diagram, algo),
    }
}

/// flowchart / 通用 stack：从 `group_arrangement` / `group_gap` / `group_align` / `group_sizing` 解析。
///
/// `group_sizing: uniform` → `TrackSizing::Equal`（与 architecture 对齐，补齐 spec §4.1 缺口）。
fn resolve_stack(diagram: &Diagram, algo: &str) -> GroupFrameSpec {
    let mut gap = 60.0_f64;
    let mut cross_align = CrossAlign::Center;
    let mut axis = Axis::Vertical;
    let mut track_sizing = TrackSizing::Fit;

    for attr in &diagram.attributes {
        match attr.key.as_str() {
            dsl::GROUP_ARRANGEMENT => {
                if let Some(s) = attr.value.as_str() {
                    axis = match s.trim().to_ascii_lowercase().as_str() {
                        "horizontal" => Axis::Horizontal,
                        // vertical 或其他：保持默认 Vertical
                        _ => Axis::Vertical,
                    };
                }
            }
            dsl::GROUP_GAP => {
                if let AttributeValue::Number(n) = &attr.value {
                    if *n > 0.0 {
                        gap = *n;
                    }
                }
            }
            dsl::GROUP_ALIGN => {
                if let Some(s) = attr.value.as_str() {
                    cross_align = match s.trim().to_ascii_lowercase().as_str() {
                        "left" => CrossAlign::Start,
                        "center" => CrossAlign::Center,
                        _ => cross_align,
                    };
                }
            }
            dsl::GROUP_SIZING => {
                if let Some(v) = attr.value.as_str() {
                    if v.trim().to_ascii_lowercase() == "uniform" {
                        track_sizing = TrackSizing::Equal;
                    }
                }
            }
            _ => {}
        }
    }

    GroupFrameSpec {
        arrangement: GroupArrangement::Stack { axis },
        track_sizing,
        cross_align,
        gap,
        padding: GroupPadding::uniform(
            crate::layout::constants::SUGIYAMA_GROUP_PADDING,
            GROUP_LABEL_HEIGHT,
        ),
        border_align: BorderAlign::None,
        quantize: resolve_quantize(diagram, algo),
    }
}

/// 解析量化规格：`enabled` = 算法白名单 ∩ `snap` 属性（默认 true）。
fn resolve_quantize(diagram: &Diagram, algo: &str) -> QuantizeSpec {
    let enabled = should_snap(algo) && diagram_snap_attribute(diagram).unwrap_or(true);
    QuantizeSpec {
        enabled,
        step: DEFAULT_QUANTIZE_STEP,
        quantize_groups: enabled,
        quantize_nodes: enabled,
    }
}

/// 读取 diagram 属性 `group_sizing` 是否为 `uniform`（与 `parse_group_sizing` 对齐）。
///
/// P1 将吸收 `parse_group_sizing`，届时此函数替换为直接调用。
fn diagram_group_sizing_is_uniform(diagram: &Diagram) -> bool {
    for attr in &diagram.attributes {
        if attr.key == dsl::GROUP_SIZING {
            if let Some(v) = attr.value.as_str() {
                return v.trim().to_ascii_lowercase() == "uniform";
            }
        }
    }
    false
}

// ─── L1 整形 Pass（P1）─────────────────────────────────────

/// `apply_group_frame` 执行报告。
#[derive(Debug, Clone, Default)]
pub struct GroupFrameReport {
    /// 参与整形的顶层 group 数量
    pub top_group_count: usize,
    /// 处理的嵌套 sibling set 数量（不含顶层；sub-frame 递归层数，见 spec §3.1/§8 P3）
    pub nested_frames_applied: usize,
    /// 是否执行了 Matrix 二维排列
    pub matrix_applied: bool,
    /// 是否执行了 Equal 拉齐
    pub equalized: bool,
    /// 是否执行了 cross_align Start 左缘对齐
    pub cross_aligned: bool,
    /// border_align 共线处理的边框数
    pub borders_aligned: usize,
    /// quantize 量化的 group 数
    pub groups_quantized: usize,
}

/// L1 Group Frame 整形 Pass。
///
/// 按 [`GroupFrameSpec`] 对 **同级 group 集合**（sibling set）施加：arrangement →
/// border_align → quantize。节点联动分级（见 spec §2.1/§3.1）：
/// - `track_sizing` / `cross_align` / `Matrix` 排列 → **必须**同步平移组内节点（跳过 PinSet）
/// - `border_align` / `quantize` 微调（≤1 step）→ **只改 `GroupLayout`**，不动节点
///
/// # 嵌套 sub-frame（spec §3.1/§8 P3）
///
/// 按 parent 分层，自顶向下逐层应用：先整形顶层 group（parent=None），再对每个含子 group
/// 的 parent 递归整形其直接子 group（sub-frame）。父层先于子层执行，保证父框已落到最终
/// 位置后，子 group 在父框内重新排列。同一 `GroupFrameSpec` 应用于所有层级。
///
/// # 确定性
///
/// 所有迭代使用排序后的 `Vec`（按 group.id 字典序），不依赖 `HashMap` 迭代序
/// （见 `AGENTS.md` §2）。sibling set 收集顺序：BFS（顶层 → 各 parent 的直接子 group），
/// 同一 parent 内按 `diagram.groups` 声明序。
///
/// # 幂等性
///
/// 同一 Spec 连续执行两次结果不变（见 spec §5.3）。
pub fn apply_group_frame(
    spec: &GroupFrameSpec,
    diagram: &Diagram,
    layout: &mut LayoutResult,
    pinned: &PinSet,
) -> GroupFrameReport {
    // 自顶向下收集 sibling sets：第一个为顶层（parent=None），后续为各 parent 的直接子 group
    let sibling_sets = collect_sibling_sets(diagram);
    let mut report = GroupFrameReport {
        top_group_count: sibling_sets.first().map(|s| s.len()).unwrap_or(0),
        ..Default::default()
    };

    for (idx, target_ids) in sibling_sets.iter().enumerate() {
        if target_ids.is_empty() {
            continue;
        }
        if idx > 0 {
            report.nested_frames_applied += 1;
        }

        // 节点 / group → 本层 target 的祖先映射（确定性：按声明序构建）
        let node_to_target = build_node_to_ancestor_in_set(diagram, target_ids);
        let group_to_target = build_group_to_ancestor_in_set(diagram, target_ids);

        // 步骤 1：按 arrangement 排列
        // - Matrix：二维网格（含 track_sizing Equal 列宽/行高 + cross_align cell 内对齐）
        // - Stack：cross_align + track_sizing 分步整形
        match &spec.arrangement {
            GroupArrangement::Matrix { rows, cols } => {
                if apply_matrix_arrangement(
                    *rows,
                    *cols,
                    spec,
                    target_ids,
                    &mut layout.groups,
                    &mut layout.nodes,
                    &node_to_target,
                    &group_to_target,
                    pinned,
                ) {
                    report.matrix_applied = true;
                }
            }
            GroupArrangement::Stack { .. } => {
                if matches!(spec.cross_align, CrossAlign::Start) {
                    if apply_cross_align_start(
                        target_ids,
                        &mut layout.groups,
                        &mut layout.nodes,
                        &node_to_target,
                        &group_to_target,
                        pinned,
                    ) {
                        report.cross_aligned = true;
                    }
                }
                if matches!(spec.track_sizing, TrackSizing::Equal) {
                    if apply_equal_sizing(
                        target_ids,
                        &mut layout.groups,
                        &mut layout.nodes,
                        &node_to_target,
                        &group_to_target,
                        pinned,
                    ) {
                        report.equalized = true;
                    }
                }
            }
        }

        // 步骤 2：border_align SharedLines（只改框，不动节点；仅作用于本层 sibling set）
        if matches!(spec.border_align, BorderAlign::SharedLines) {
            report.borders_aligned +=
                apply_border_align_for(target_ids, &mut layout.groups, spec.quantize.step);
        }

        // 步骤 3：quantize groups（只改框，不动节点；仅作用于本层 sibling set）
        if spec.quantize.enabled && spec.quantize.quantize_groups {
            report.groups_quantized +=
                apply_group_quantize_for(target_ids, &mut layout.groups, spec.quantize.step);
            // quantize 后可能破坏边框共线，再跑一次 border_align
            if matches!(spec.border_align, BorderAlign::SharedLines) {
                report.borders_aligned +=
                    apply_border_align_for(target_ids, &mut layout.groups, spec.quantize.step);
            }
        }

        // 步骤 4：resolve sibling overlaps（安全网）
        // recompute_group_bounds + quantize 可能使 group 宽度超出原始 macro block 宽度
        // （quantize floor 左 / ceil 右，最多扩展 2*step），导致同行 group 重叠。
        // 此 pass 检测并消除 sibling 间的实际重叠，同步平移组内节点与嵌套 group 框。
        resolve_sibling_overlaps(
            target_ids,
            &mut layout.groups,
            &mut layout.nodes,
            &node_to_target,
            &group_to_target,
            &spec.arrangement,
            spec.gap,
            pinned,
        );
    }

    report
}

/// 收集 sibling sets（同级 group 集合），自顶向下 BFS 顺序。
///
/// 返回顺序：第一个为顶层 group（`parent_id == None`），后续为各 parent 的直接子 group
/// 集合。父层先于子层，保证 sub-frame 递归时父框已就位。同一 parent 内按
/// `diagram.groups` 声明序（确定性）。
///
/// 利用 `Group::child_group_ids` 做 BFS：顶层入队后逐层展开子 group。
fn collect_sibling_sets(diagram: &Diagram) -> Vec<Vec<String>> {
    use std::collections::VecDeque;

    let mut sets: Vec<Vec<String>> = Vec::new();
    let mut queue: VecDeque<String> = VecDeque::new();

    // 顶层（parent=None）— 声明序
    let top: Vec<String> = diagram
        .groups
        .iter()
        .filter(|g| g.parent_id.is_none())
        .map(|g| g.id.as_str().to_string())
        .collect();
    if !top.is_empty() {
        for id in &top {
            queue.push_back(id.clone());
        }
        sets.push(top);
    }

    // BFS：对每个出队的 parent，收集其直接子 group
    while let Some(parent_id) = queue.pop_front() {
        let parent = match diagram.find_group(&parent_id) {
            Some(g) => g,
            None => continue,
        };
        // child_group_ids 已是声明序（解析期填充）
        let children: Vec<String> = parent
            .child_group_ids
            .iter()
            .map(|c| c.as_str().to_string())
            .collect();
        if !children.is_empty() {
            for id in &children {
                queue.push_back(id.clone());
            }
            sets.push(children);
        }
    }

    sets
}

/// 节点 → 本层 target 的祖先映射。
///
/// 对每个节点，沿 `group_id` → `parent_id` 链向上找到第一个属于 `target_ids` 的 group。
/// 用于 sub-frame 平移节点时确定节点归属（target group 的所有后代节点随 target 平移）。
///
/// 确定性：按 `diagram.entities` 声明序构建。
fn build_node_to_ancestor_in_set(
    diagram: &Diagram,
    target_ids: &[String],
) -> HashMap<String, String> {
    let target_set: HashSet<&str> = target_ids.iter().map(|s| s.as_str()).collect();
    let mut map = HashMap::new();
    for entity in &diagram.entities {
        let Some(start_gid) = entity.group_id.as_ref() else { continue; };
        let mut cur = start_gid.as_str().to_string();
        loop {
            if target_set.contains(cur.as_str()) {
                map.insert(entity.id.as_str().to_string(), cur);
                break;
            }
            let Some(g) = diagram.find_group(&cur) else { break; };
            match &g.parent_id {
                Some(p) => cur = p.as_str().to_string(),
                None => break,
            }
        }
    }
    map
}

/// group → 本层 target 的祖先映射。
///
/// 对每个 group，沿 `parent_id` 链向上找到第一个属于 `target_ids` 的 group（含自身）。
/// 用于 sub-frame 平移 target group 时同步平移其所有后代 group 框。
///
/// 确定性：按 `diagram.groups` 声明序构建。
fn build_group_to_ancestor_in_set(
    diagram: &Diagram,
    target_ids: &[String],
) -> HashMap<String, String> {
    let target_set: HashSet<&str> = target_ids.iter().map(|s| s.as_str()).collect();
    let mut map = HashMap::new();
    for group in &diagram.groups {
        let mut cur = group.id.as_str().to_string();
        loop {
            if target_set.contains(cur.as_str()) {
                map.insert(group.id.as_str().to_string(), cur);
                break;
            }
            let Some(g) = diagram.find_group(&cur) else { break; };
            match &g.parent_id {
                Some(p) => cur = p.as_str().to_string(),
                None => break,
            }
        }
    }
    map
}

/// cross_align Start：所有顶层 group 左缘对齐到全局 `min(x)`，同步平移组内节点与嵌套 group 框。
///
/// **按行（y）分组对齐**：同一 y（同一 macro rank）的 group 已被 `position_macro_blocks`
/// 水平并排放置，不能强制左缘对齐到同一 x（否则同 rank group 会完全重叠）。
/// 因此按 y 将顶层 group 分行，每行整体平移使该行 `min(x)` 等于全局 `min(x)`，
/// 行内 group 保持相对 x 位置不变。不同 y 的行各自独立平移到全局 `min(x)`，
/// 形成跨 rank 的整齐左缘。
///
/// **节点联动分级**（spec §2.1/§3.1）：cross_align 属于「必须同步平移节点」级别，
/// 因此嵌套 group 框也必须同步平移，否则嵌套 group 不再包含其成员节点。
///
/// 返回 `true` 表示执行了平移。
fn apply_cross_align_start(
    top_ids: &[String],
    groups: &mut HashMap<String, GroupLayout>,
    nodes: &mut HashMap<String, NodeLayout>,
    node_to_top: &HashMap<String, String>,
    group_to_top: &HashMap<String, String>,
    pinned: &PinSet,
) -> bool {
    if top_ids.len() < 2 {
        return false;
    }

    // 全局 min(x)：所有行的对齐目标
    let target_left = top_ids
        .iter()
        .filter_map(|id| groups.get(id).map(|g| g.x))
        .fold(f64::INFINITY, f64::min);
    if !target_left.is_finite() {
        return false;
    }

    // 按 y 分行（同一 macro rank 的 group 在同一行）。
    // 确定性：先按 top_ids 声明序收集，再按 y 排序处理。
    let mut rows: Vec<(f64, Vec<String>)> = Vec::new();
    for top_id in top_ids {
        if let Some(g) = groups.get(top_id) {
            // 容差 0.5px：同一 rank 的 group y 相同
            let row_idx = rows.iter().position(|(row_y, _)| (row_y - g.y).abs() < 0.5);
            match row_idx {
                Some(idx) => rows[idx].1.push(top_id.clone()),
                None => rows.push((g.y, vec![top_id.clone()])),
            }
        }
    }
    rows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let mut shifted = false;
    for (_, row_ids) in &rows {
        // 该行 min(x)
        let row_min_x = row_ids
            .iter()
            .filter_map(|id| groups.get(id).map(|g| g.x))
            .fold(f64::INFINITY, f64::min);
        if !row_min_x.is_finite() {
            continue;
        }

        let shift = target_left - row_min_x;
        if shift.abs() < 0.5 {
            continue;
        }

        // 确定性：按 row_ids 顺序（diagram.groups 声明序）
        for top_id in row_ids {
            // 平移顶层 group 框
            if let Some(g) = groups.get_mut(top_id) {
                g.x += shift;
            }

            // 平移组内节点（跳过 PinSet 保护的节点）
            for (node_id, nl) in nodes.iter_mut() {
                if node_to_top.get(node_id) == Some(top_id) && !pinned.is_x_pinned(node_id) {
                    nl.x += shift;
                }
            }

            // 平移嵌套 group 框（top_id 的所有后代 group，排除 top_id 自身）
            // 确定性：按 group id 字典序
            let mut nested_ids: Vec<String> = groups
                .keys()
                .filter(|gid| {
                    gid.as_str() != top_id.as_str()
                        && group_to_top.get(*gid).map(String::as_str) == Some(top_id.as_str())
                })
                .cloned()
                .collect();
            nested_ids.sort();
            for gid in nested_ids {
                if let Some(g) = groups.get_mut(&gid) {
                    g.x += shift;
                }
            }
        }
        shifted = true;
    }

    shifted
}

/// 消除同级 group 间的实际重叠（安全网 pass）。
///
/// # 背景
///
/// `recompute_group_bounds` 从实际节点位置重算 group 宽度，`apply_group_quantize_for`
/// 进一步 floor 左 / ceil 右（最多扩展 `2*step`）。这些后处理可能使 group 宽度超出
/// `position_macro_blocks` 放置时使用的 `intra.content_width + padding.x_delta`，
/// 导致同行（同 y band）的 group 在 x 方向重叠。
///
/// # 算法
///
/// - `Stack(Horizontal)`：按 x 排序，左→右扫描，若相邻 group 在 x 和 y 上均有重叠，
///   将后者右移 `prev_right + gap - curr_left`，同步平移组内节点与嵌套 group 框。
/// - `Stack(Vertical)`：按 y 排序，上→下扫描，若相邻 group 在 x 和 y 上均有重叠，
///   将后者下移 `prev_bottom + gap - curr_top`，同步平移。
/// - `Matrix`：不处理（二维排列的重叠应由 arrangement 本身保证）。
///
/// # 确定性
///
/// 排序使用 `partial_cmp` + group id 字典序 tie-breaker，不依赖 HashMap 迭代序。
fn resolve_sibling_overlaps(
    target_ids: &[String],
    groups: &mut HashMap<String, GroupLayout>,
    nodes: &mut HashMap<String, NodeLayout>,
    node_to_target: &HashMap<String, String>,
    group_to_target: &HashMap<String, String>,
    arrangement: &GroupArrangement,
    gap: f64,
    pinned: &PinSet,
) {
    if target_ids.len() < 2 {
        return;
    }

    match arrangement {
        GroupArrangement::Matrix { .. } => return,
        GroupArrangement::Stack { axis } => {
            match axis {
                Axis::Horizontal => {
                    // Architecture 布局：同行 group 沿 x 并排，不同行沿 y 堆叠。
                    // 两步消除重叠：
                    // 1. 同行内按 x 排序消除 x 重叠
                    // 2. 行间按 y 排序消除 y 重叠

                    // 按 y 分行（容差 0.5px）
                    let mut rows: Vec<(f64, Vec<String>)> = Vec::new();
                    for id in target_ids {
                        let Some(g) = groups.get(id) else { continue };
                        let row_idx = rows
                            .iter()
                            .position(|(row_y, _)| (row_y - g.y).abs() < 0.5);
                        match row_idx {
                            Some(idx) => rows[idx].1.push(id.clone()),
                            None => rows.push((g.y, vec![id.clone()])),
                        }
                    }
                    rows.sort_by(|a, b| {
                        a.0.partial_cmp(&b.0)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    });

                    // 步骤 1：同行内消除 x 重叠
                    for (_, row_ids) in &rows {
                        if row_ids.len() < 2 {
                            continue;
                        }
                        let mut sorted: Vec<String> = row_ids.to_vec();
                        sorted.sort_by(|a, b| {
                            let xa = groups.get(a).map(|g| g.x).unwrap_or(0.0);
                            let xb = groups.get(b).map(|g| g.x).unwrap_or(0.0);
                            xa.partial_cmp(&xb)
                                .unwrap_or(std::cmp::Ordering::Equal)
                                .then_with(|| a.cmp(b))
                        });

                        for i in 1..sorted.len() {
                            let (prev_id, curr_id) =
                                (sorted[i - 1].clone(), sorted[i].clone());
                            let prev_right = groups
                                .get(&prev_id)
                                .map(|g| g.x + g.width)
                                .unwrap_or(0.0);
                            let curr_x =
                                groups.get(&curr_id).map(|g| g.x).unwrap_or(0.0);

                            if prev_right > curr_x + 0.5 {
                                let shift = prev_right + gap - curr_x;
                                if shift > 0.5 {
                                    shift_target_horizontally(
                                        &curr_id,
                                        shift,
                                        groups,
                                        nodes,
                                        node_to_target,
                                        group_to_target,
                                        pinned,
                                    );
                                }
                            }
                        }
                    }

                    // 步骤 2：行间消除 y 重叠
                    // 收集每行的 y 范围（min_y, max_y_bottom）
                    let mut row_bounds: Vec<(f64, f64)> = Vec::new();
                    for (row_y, row_ids) in &rows {
                        let min_y = *row_y;
                        let max_bottom = row_ids
                            .iter()
                            .filter_map(|id| groups.get(id).map(|g| g.y + g.height))
                            .fold(0.0_f64, f64::max);
                        row_bounds.push((min_y, max_bottom));
                    }

                    for i in 1..row_bounds.len() {
                        let prev_bottom = row_bounds[i - 1].1;
                        let curr_top = row_bounds[i].0;
                        if prev_bottom > curr_top + 0.5 {
                            let shift = prev_bottom + gap - curr_top;
                            if shift > 0.5 {
                                // 下推当前行所有 group
                                for id in &rows[i].1 {
                                    shift_target_vertically(
                                        id,
                                        shift,
                                        groups,
                                        nodes,
                                        node_to_target,
                                        group_to_target,
                                        pinned,
                                    );
                                }
                                // 更新行边界
                                row_bounds[i].0 += shift;
                                row_bounds[i].1 += shift;
                            }
                        }
                    }
                }
                Axis::Vertical => {
                    // 按 y 排序，消除 y 重叠（仅当 x 也有重叠时才处理）
                    let mut sorted: Vec<String> = target_ids.to_vec();
                    sorted.sort_by(|a, b| {
                        let ya = groups.get(a).map(|g| g.y).unwrap_or(0.0);
                        let yb = groups.get(b).map(|g| g.y).unwrap_or(0.0);
                        ya.partial_cmp(&yb)
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then_with(|| a.cmp(b))
                    });

                    for i in 1..sorted.len() {
                        let (prev_id, curr_id) = (sorted[i - 1].clone(), sorted[i].clone());
                        let (prev_x, prev_w, prev_y, prev_h) = match groups.get(&prev_id) {
                            Some(g) => (g.x, g.width, g.y, g.height),
                            None => continue,
                        };
                        let (curr_x, curr_w, curr_y) = match groups.get(&curr_id) {
                            Some(g) => (g.x, g.width, g.y),
                            None => continue,
                        };

                        // x 方向是否有重叠
                        let x_overlap =
                            (prev_x + prev_w).min(curr_x + curr_w) - prev_x.max(curr_x);
                        if x_overlap <= 1.0 {
                            continue;
                        }

                        // y 方向是否有重叠
                        let prev_bottom = prev_y + prev_h;
                        if prev_bottom <= curr_y + 0.5 {
                            continue;
                        }

                        let shift = prev_bottom + gap - curr_y;
                        if shift < 0.5 {
                            continue;
                        }

                        shift_target_vertically(
                            &curr_id,
                            shift,
                            groups,
                            nodes,
                            node_to_target,
                            group_to_target,
                            pinned,
                        );
                    }
                }
            }
        }
    }
}

/// 将 target group 及其所有后代节点 / 嵌套 group 框水平平移 `dx`。
fn shift_target_horizontally(
    target_id: &str,
    dx: f64,
    groups: &mut HashMap<String, GroupLayout>,
    nodes: &mut HashMap<String, NodeLayout>,
    node_to_target: &HashMap<String, String>,
    group_to_target: &HashMap<String, String>,
    pinned: &PinSet,
) {
    // 平移 target group 自身
    if let Some(g) = groups.get_mut(target_id) {
        g.x += dx;
    }

    // 平移组内节点（跳过 PinSet x-pinned 节点）
    for (node_id, nl) in nodes.iter_mut() {
        if node_to_target.get(node_id).map(String::as_str) == Some(target_id)
            && !pinned.is_x_pinned(node_id)
        {
            nl.x += dx;
        }
    }

    // 平移嵌套 group 框（target 的所有后代 group，排除 target 自身）
    let mut nested_ids: Vec<String> = groups
        .keys()
        .filter(|gid| {
            gid.as_str() != target_id
                && group_to_target.get(*gid).map(String::as_str) == Some(target_id)
        })
        .cloned()
        .collect();
    nested_ids.sort();
    for gid in nested_ids {
        if let Some(g) = groups.get_mut(&gid) {
            g.x += dx;
        }
    }
}

/// 将 target group 及其所有后代节点 / 嵌套 group 框垂直平移 `dy`。
fn shift_target_vertically(
    target_id: &str,
    dy: f64,
    groups: &mut HashMap<String, GroupLayout>,
    nodes: &mut HashMap<String, NodeLayout>,
    node_to_target: &HashMap<String, String>,
    group_to_target: &HashMap<String, String>,
    pinned: &PinSet,
) {
    if let Some(g) = groups.get_mut(target_id) {
        g.y += dy;
    }

    for (node_id, nl) in nodes.iter_mut() {
        if node_to_target.get(node_id).map(String::as_str) == Some(target_id)
            && !pinned.is_y_pinned(node_id)
        {
            nl.y += dy;
        }
    }

    let mut nested_ids: Vec<String> = groups
        .keys()
        .filter(|gid| {
            gid.as_str() != target_id
                && group_to_target.get(*gid).map(String::as_str) == Some(target_id)
        })
        .cloned()
        .collect();
    nested_ids.sort();
    for gid in nested_ids {
        if let Some(g) = groups.get_mut(&gid) {
            g.y += dy;
        }
    }
}

/// track_sizing Equal：所有顶层 group 拉齐到 `max(width)`，组内节点水平居中。
///
/// **节点联动分级**（spec §2.1/§3.1）：track_sizing 属于「必须同步平移节点」级别，
/// 因此嵌套 group 框也必须同步平移，否则嵌套 group 不再包含其成员节点。
///
/// 返回 `true` 表示执行了拉齐。
fn apply_equal_sizing(
    top_ids: &[String],
    groups: &mut HashMap<String, GroupLayout>,
    nodes: &mut HashMap<String, NodeLayout>,
    node_to_top: &HashMap<String, String>,
    group_to_top: &HashMap<String, String>,
    pinned: &PinSet,
) -> bool {
    if top_ids.is_empty() {
        return false;
    }

    let max_width = top_ids
        .iter()
        .filter_map(|id| groups.get(id).map(|g| g.width))
        .fold(0.0_f64, f64::max);
    if max_width <= f64::EPSILON {
        return false;
    }

    let mut equalized = false;
    for top_id in top_ids {
        let Some(group) = groups.get(top_id) else {
            continue;
        };
        let extra_w = max_width - group.width;
        if extra_w <= f64::EPSILON {
            continue;
        }

        // 设置顶层 group 宽度
        if let Some(g) = groups.get_mut(top_id) {
            g.width = max_width;
        }

        // 组内节点水平居中（跳过 PinSet 保护的节点）
        let half = extra_w / 2.0;
        for (node_id, nl) in nodes.iter_mut() {
            if node_to_top.get(node_id) == Some(top_id) && !pinned.is_x_pinned(node_id) {
                nl.x += half;
            }
        }

        // 嵌套 group 框同步平移 half（保持与组内节点相对位置一致）
        // 确定性：按 group id 字典序
        let mut nested_ids: Vec<String> = groups
            .keys()
            .filter(|gid| {
                gid.as_str() != top_id.as_str()
                    && group_to_top.get(*gid).map(String::as_str) == Some(top_id.as_str())
            })
            .cloned()
            .collect();
        nested_ids.sort();
        for gid in nested_ids {
            if let Some(g) = groups.get_mut(&gid) {
                g.x += half;
            }
        }
        equalized = true;
    }

    equalized
}

/// Matrix 二维排列：将顶层 group 放入行优先网格，按 `track_sizing` 决定列宽/行高，
/// 按 `cross_align` 决定 cell 内对齐，按 `gap` 累加间距。
///
/// **节点联动分级**（spec §2.1/§3.1）：Matrix 排列属于「必须同步平移节点」级别，
/// 组内节点与嵌套 group 框随顶层 group 整体平移（跳过 PinSet 保护的节点）。
///
/// # 确定性
///
/// - group 顺序：按当前几何 `(y, x)` 排序（保持主布局产出的近似序），tie-break 用 id 字典序
/// - 行列推断：见 [`infer_matrix_dims`]
///
/// # 幂等性
///
/// 排列后 group 已落在网格格点；第二次执行时 `(y, x)` 序不变、列宽/行高不变、
/// cell 内偏移不变，故 `dx`/`dy` ≈ 0，为 no-op。
fn apply_matrix_arrangement(
    rows: Option<u32>,
    cols: Option<u32>,
    spec: &GroupFrameSpec,
    top_ids: &[String],
    groups: &mut HashMap<String, GroupLayout>,
    nodes: &mut HashMap<String, NodeLayout>,
    node_to_top: &HashMap<String, String>,
    group_to_top: &HashMap<String, String>,
    pinned: &PinSet,
) -> bool {
    let n = top_ids.len();
    if n == 0 {
        return false;
    }

    // 1. 推断行列
    let (n_rows, n_cols) = infer_matrix_dims(rows, cols, n);

    // 2. 按当前几何 (y, x) 排序（行优先），tie-break 用 id 字典序（确定性）
    let mut ordered: Vec<String> = top_ids.to_vec();
    ordered.sort_by(|a, b| {
        let ga = groups.get(a);
        let gb = groups.get(b);
        let ya = ga.map(|g| g.y).unwrap_or(0.0);
        let yb = gb.map(|g| g.y).unwrap_or(0.0);
        let xa = ga.map(|g| g.x).unwrap_or(0.0);
        let xb = gb.map(|g| g.x).unwrap_or(0.0);
        ya.partial_cmp(&yb)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| xa.partial_cmp(&xb).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.cmp(b))
    });

    // 3. 计算每列最大宽度、每行最大高度（Fit = 贴合内容）
    let mut col_widths = vec![0.0_f64; n_cols];
    let mut row_heights = vec![0.0_f64; n_rows];
    for (i, id) in ordered.iter().enumerate() {
        let r = i / n_cols;
        let c = i % n_cols;
        if let Some(g) = groups.get(id) {
            col_widths[c] = col_widths[c].max(g.width);
            row_heights[r] = row_heights[r].max(g.height);
        }
    }

    // track_sizing Equal：所有列等宽、所有行等高（取 max）
    if matches!(spec.track_sizing, TrackSizing::Equal) {
        let max_w = col_widths.iter().copied().fold(0.0_f64, f64::max).max(1.0);
        let max_h = row_heights.iter().copied().fold(0.0_f64, f64::max).max(1.0);
        col_widths.fill(max_w);
        row_heights.fill(max_h);
    }
    // track_sizing Fixed：固定列宽/行高
    if let TrackSizing::Fixed(v) = spec.track_sizing {
        let v = v.max(1.0);
        col_widths.fill(v);
        row_heights.fill(v);
    }

    // 4. 网格原点：所有 group 的最小 x、最小 y
    let origin_x = ordered
        .iter()
        .filter_map(|id| groups.get(id).map(|g| g.x))
        .fold(f64::INFINITY, f64::min);
    let origin_y = ordered
        .iter()
        .filter_map(|id| groups.get(id).map(|g| g.y))
        .fold(f64::INFINITY, f64::min);
    if !origin_x.is_finite() || !origin_y.is_finite() {
        return false;
    }

    // 5. 累加 gap 得到每列 x、每行 y
    let gap = spec.gap.max(0.0);
    let mut col_x = vec![origin_x; n_cols];
    let mut row_y = vec![origin_y; n_rows];
    for c in 1..n_cols {
        col_x[c] = col_x[c - 1] + col_widths[c - 1] + gap;
    }
    for r in 1..n_rows {
        row_y[r] = row_y[r - 1] + row_heights[r - 1] + gap;
    }

    // 6. 放置每个 group 到对应 cell，按 cross_align 决定 cell 内偏移
    let mut applied = false;
    for (i, id) in ordered.iter().enumerate() {
        let r = i / n_cols;
        let c = i % n_cols;
        let Some(g) = groups.get(id) else { continue; };

        let (new_x, new_y) = match spec.cross_align {
            CrossAlign::Start | CrossAlign::Stretch => (col_x[c], row_y[r]),
            CrossAlign::Center => (
                col_x[c] + (col_widths[c] - g.width) / 2.0,
                row_y[r] + (row_heights[r] - g.height) / 2.0,
            ),
            CrossAlign::End => (
                col_x[c] + (col_widths[c] - g.width),
                row_y[r] + (row_heights[r] - g.height),
            ),
        };

        let dx = new_x - g.x;
        let dy = new_y - g.y;
        if dx.abs() < 0.5 && dy.abs() < 0.5 {
            continue;
        }

        // 平移顶层 group 框
        if let Some(g) = groups.get_mut(id) {
            g.x = new_x;
            g.y = new_y;
        }

        // 平移组内节点（跳过 PinSet 保护的节点）
        for (node_id, nl) in nodes.iter_mut() {
            if node_to_top.get(node_id) == Some(id) {
                if !pinned.is_x_pinned(node_id) {
                    nl.x += dx;
                }
                if !pinned.is_y_pinned(node_id) {
                    nl.y += dy;
                }
            }
        }

        // 平移嵌套 group 框（确定性：按 group id 字典序）
        let mut nested_ids: Vec<String> = groups
            .keys()
            .filter(|gid| {
                gid.as_str() != id.as_str()
                    && group_to_top.get(*gid).map(String::as_str) == Some(id.as_str())
            })
            .cloned()
            .collect();
        nested_ids.sort();
        for gid in nested_ids {
            if let Some(g) = groups.get_mut(&gid) {
                g.x += dx;
                g.y += dy;
            }
        }
        applied = true;
    }

    applied
}

/// 推断 Matrix 行列数。
///
/// - 两者均指定：直接使用（可能产生空 cell）
/// - 仅指定 `rows`：`cols = ceil(n / rows)`
/// - 仅指定 `cols`：`rows = ceil(n / cols)`
/// - 均未指定：自动推断，`cols = ceil(sqrt(n))`，`rows = ceil(n / cols)`（接近正方形）
fn infer_matrix_dims(rows: Option<u32>, cols: Option<u32>, n: usize) -> (usize, usize) {
    match (rows, cols) {
        (Some(r), Some(c)) => ((r as usize).max(1), (c as usize).max(1)),
        (Some(r), None) => {
            let r = (r as usize).max(1);
            let c = ((n + r - 1) / r).max(1);
            (r, c)
        }
        (None, Some(c)) => {
            let c = (c as usize).max(1);
            let r = ((n + c - 1) / c).max(1);
            (r, c)
        }
        (None, None) => {
            let c = ((n as f64).sqrt().ceil() as usize).max(1);
            let r = ((n + c - 1) / c).max(1);
            (r, c)
        }
    }
}

/// border_align SharedLines：同侧边框共线（左/上），只改 `GroupLayout` 不动节点。
///
/// 仅作用于 `ids` 指定的 sibling set（同级 group 之间共线），不影响其他层级。
///
/// 对齐左边缘时同步调整 width（保持右边缘不变），对齐上边缘时同步调整 height
/// （保持下边缘不变）。聚类阈值 = `step`，簇内取中位数（确定性）。
///
/// 返回对齐的边框数。
fn apply_border_align_for(
    ids: &[String],
    groups: &mut HashMap<String, GroupLayout>,
    step: f64,
) -> usize {
    if ids.len() < 2 || step <= f64::EPSILON {
        return 0;
    }

    // 确定性：按 id 字典序
    let mut sorted: Vec<String> = ids.to_vec();
    sorted.sort();

    let left_count = align_border_set(groups, &sorted, step, |g| g.x, |g, v| {
        let delta = v - g.x;
        g.width -= delta;
        g.x = v;
    });
    let top_count = align_border_set(groups, &sorted, step, |g| g.y, |g, v| {
        let delta = v - g.y;
        g.height -= delta;
        g.y = v;
    });

    left_count + top_count
}

/// 对齐同一侧边框：检测在 1 个 step 内的边框，统一到中位数（确定性）。
///
/// 返回对齐的边框数。
fn align_border_set(
    groups: &mut HashMap<String, GroupLayout>,
    ids: &[String],
    step: f64,
    get: impl Fn(&GroupLayout) -> f64,
    set: impl Fn(&mut GroupLayout, f64),
) -> usize {
    // 收集 (id, value)，按 value 升序 + id 字典序 tie-break
    let mut entries: Vec<(String, f64)> = ids
        .iter()
        .map(|id| (id.clone(), get(&groups[id])))
        .collect();
    entries.sort_by(|a, b| {
        a.1.partial_cmp(&b.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    // 聚类：相邻值差 < step 归为同一组
    let mut clusters: Vec<Vec<(String, f64)>> = Vec::new();
    for entry in entries {
        if let Some(last_cluster) = clusters.last_mut() {
            let cluster_min = last_cluster[0].1;
            if (entry.1 - cluster_min).abs() < step {
                last_cluster.push(entry);
                continue;
            }
        }
        clusters.push(vec![entry]);
    }

    // 对多于 1 个元素的聚类，统一到中位数（确定性）
    let mut aligned = 0usize;
    for cluster in &clusters {
        if cluster.len() < 2 {
            continue;
        }
        let mut values: Vec<f64> = cluster.iter().map(|(_, v)| *v).collect();
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = values[values.len() / 2];
        for (id, _) in cluster {
            if let Some(g) = groups.get_mut(id) {
                set(g, median);
                aligned += 1;
            }
        }
    }

    aligned
}

/// quantize groups：group 四边量化到像素网格（floor 原点，ceil 远端）。
///
/// 只改 `GroupLayout`，不动节点（微调 ≤1 step，节点仍在框内）。
/// 仅作用于 `ids` 指定的 sibling set。
///
/// 返回量化的 group 数。
fn apply_group_quantize_for(
    ids: &[String],
    groups: &mut HashMap<String, GroupLayout>,
    step: f64,
) -> usize {
    if ids.is_empty() || step <= f64::EPSILON {
        return 0;
    }

    let mut quantized = 0usize;
    for id in ids {
        let Some(group) = groups.get_mut(id) else { continue; };
        let right = group.x + group.width;
        let bottom = group.y + group.height;
        let new_x = snap_floor(group.x, step);
        let new_y = snap_floor(group.y, step);
        let new_right = snap_ceil(right, step);
        let new_bottom = snap_ceil(bottom, step);
        let new_width = (new_right - new_x).max(step);
        let new_height = (new_bottom - new_y).max(step);

        if (new_x - group.x).abs() > f64::EPSILON
            || (new_y - group.y).abs() > f64::EPSILON
            || (new_width - group.width).abs() > f64::EPSILON
            || (new_height - group.height).abs() > f64::EPSILON
        {
            quantized += 1;
        }

        group.x = new_x;
        group.y = new_y;
        group.width = new_width;
        group.height = new_height;
    }

    quantized
}

/// 从节点位置重算 group bounds（L3→L1 数据流桥梁）。
///
/// 供管线在 L3 node snap 之后、L1 group frame 之前调用。
pub fn recompute_group_bounds(
    diagram: &Diagram,
    layout: &mut LayoutResult,
    padding: GroupPadding,
) {
    let bounds_padding = BoundsGroupPadding {
        x: padding.x,
        y_top: padding.y_top,
        x_delta: padding.x_delta,
        y_delta: padding.y_delta,
    };
    layout.groups = compute_group_bounds(diagram, &layout.nodes, bounds_padding);
}

/// 按算法返回 Group Frame 使用的 padding（与 `grid_snap::refresh_layout_bounds` 对齐）。
///
/// - `architecture`：非对称 padding (28, 48, 56, 76)
/// - 其他：`uniform(group_padding, 16.0)`（header_height=16，与 `refresh_layout_bounds` 一致）
pub fn group_padding_for_algo(algo: &str, group_padding: f64) -> GroupPadding {
    if algo == "architecture" {
        GroupPadding::architecture_v2()
    } else {
        GroupPadding::uniform(group_padding, 16.0)
    }
}

#[cfg(test)]
#[path = "group_frame_tests.rs"]
mod tests;
