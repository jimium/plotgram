//! Group Frame Spec — 组间 Frame 规格类型与解析。
//!
//! 本文件从 `mod.rs` 拆分而来，仅做代码搬家，无行为变更。

use crate::ast::{AttributeValue, Diagram};
use crate::layout::constants::SUGIYAMA_GROUP_PADDING;
use crate::layout::grid_snap::diagram_snap_attribute;
use crate::layout::node::common::group_bounds::GroupPadding;
use crate::types::standard_attr_keys::diagram as dsl;
use std::collections::HashMap;

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
    Matrix {
        rows: Option<u32>,
        cols: Option<u32>,
    },
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
}

impl Default for QuantizeSpec {
    fn default() -> Self {
        Self {
            enabled: true,
            step: 8.0,
            quantize_groups: true,
        }
    }
}

// ─── 解析 ─────────────────────────────────────────────────

/// 默认量化步长。
const DEFAULT_QUANTIZE_STEP: f64 = 8.0;

/// group 标签高度（与 `architecture_v2::layout::GROUP_LABEL_HEIGHT` 一致）。
const GROUP_LABEL_HEIGHT: f64 = 20.0;

/// architecture 顶层 group 水平间距（与 `architecture_v2::layout::GROUP_GAP_X` 一致）。
const ARCH_GROUP_GAP: f64 = 40.0;

/// flowchart / 通用 stack 默认 group 间距。
const FLOWCHART_GROUP_GAP: f64 = 48.0;

/// 从 diagram 属性 + 算法名解析 [`GroupFrameSpec`]。
///
/// # 算法默认值
///
/// | 算法 | arrangement | track_sizing | cross_align | gap | border_align |
/// |------|-------------|--------------|-------------|-----|--------------|
/// | `architecture` | `Stack(H)` | `Equal`（`group_frame { track: fit }` 可退回） | `Center` | 40.0 | `SharedLines` |
/// | `flowchart` | `Stack(V)` | `Fit` | `Center` | 48.0 | `None` |
/// | 其他含 group 算法 | `Stack(V)` | `Fit` | `Center` | 48.0 | `None` |
///
/// `quantize.enabled` 由 `snap` 属性 + 节点 snap 是否启用决定。
///
/// # `group_frame:` 配置块
///
/// 若 diagram 声明了 `group_frame: stack { … }`、`group_frame: matrix { … }`，
/// 或以场景短名 `strips` / `fit` / `lanes` / `stages` / `tiles`（可带 `{ … }` 覆盖），
/// 则以配置覆盖算法默认值；未声明的字段保留算法默认。
/// 组间几何的唯一 DSL 入口；旧 `group_sizing` / `group_arrangement` 等已移除。
pub fn resolve_group_frame_spec(diagram: &Diagram, algo: &str) -> GroupFrameSpec {
    // 优先消费 `group_frame:` 配置块（覆盖算法默认值）
    if let Some(spec) = resolve_from_group_frame_config(diagram, algo) {
        return spec;
    }
    if algo == "architecture" {
        resolve_architecture(diagram)
    } else {
        // flowchart / er / sugiyama-v2 等含 group 的算法走通用 stack 解析
        resolve_stack(diagram)
    }
}

/// 从 `group_frame: stack { ... }` / 场景短名解析 Spec，覆盖算法默认值。
///
/// 配置块选项：
/// - `axis`: `"horizontal"` | `"vertical"`（stack 排列轴）
/// - `gap`: number（组间间距）
/// - `track`: `"fit"` | `"equal"` | `"uniform"`（track 尺寸策略）
/// - `cross`: `"start"` | `"center"` | `"end"` | `"stretch"`（交叉轴对齐）
/// - `border`: `"none"` | `"shared"` | `"shared_lines"`（边框共线策略）
/// - `snap`: number（量化步长）或 boolean（开关）
/// - `rows` / `cols`: matrix / `tiles` 网格尺寸
///
/// 场景短名（`strips` / `fit` / `lanes` / `stages` / `tiles`）先展开默认组合，
/// 再用 `{ … }` 覆盖单项。返回 `None` 表示未声明 `group_frame`。
fn resolve_from_group_frame_config(diagram: &Diagram, algo: &str) -> Option<GroupFrameSpec> {
    let attr = diagram
        .attributes
        .iter()
        .find(|a| a.key == dsl::GROUP_FRAME)?;
    let (arrangement_algo, options) = match &attr.value {
        AttributeValue::Config { algo, options } => (algo.as_str(), options),
        // `group_frame: stack` / `group_frame: strips`（无选项块）
        AttributeValue::String(s) => (s.as_str(), &HashMap::new()),
        _ => return None,
    };

    // 以算法默认 Spec 为基底，逐字段覆盖
    let mut spec = if algo == "architecture" {
        resolve_architecture(diagram)
    } else {
        resolve_stack(diagram)
    };

    let name = arrangement_algo.to_ascii_lowercase();
    let preset_applied = apply_group_frame_preset(&name, &mut spec);

    // arrangement（algo 字段）
    match name.as_str() {
        "stack" => {
            if let Some(axis) = read_str_option(options, "axis") {
                spec.arrangement = GroupArrangement::Stack {
                    axis: parse_stack_axis(axis),
                };
            }
        }
        "matrix" => {
            let rows = read_num_option(options, "rows").map(|n| n as u32);
            let cols = read_num_option(options, "cols").map(|n| n as u32);
            spec.arrangement = GroupArrangement::Matrix { rows, cols };
        }
        _ if preset_applied => {
            // 短名展开后仍允许覆盖 axis / rows / cols
            match &mut spec.arrangement {
                GroupArrangement::Stack { axis } => {
                    if let Some(a) = read_str_option(options, "axis") {
                        *axis = parse_stack_axis(a);
                    }
                }
                GroupArrangement::Matrix { rows, cols } => {
                    if let Some(r) = read_num_option(options, "rows") {
                        *rows = Some(r as u32);
                    }
                    if let Some(c) = read_num_option(options, "cols") {
                        *cols = Some(c as u32);
                    }
                }
            }
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
            }
            AttributeValue::Number(step) => {
                spec.quantize.enabled = true;
                spec.quantize.step = *step;
                spec.quantize.quantize_groups = true;
            }
            _ => {}
        }
    }

    Some(spec)
}

/// 是否由 DSL 显式声明了 Equal track 契约。
///
/// 与算法默认值分开：末尾重申 L1 尺寸只应作用于用户声明的契约，不能把默认
/// architecture Equal 扩散为所有图的最终坐标重写。
pub(crate) fn has_explicit_equal_track(diagram: &Diagram) -> bool {
    let Some(attr) = diagram
        .attributes
        .iter()
        .find(|attribute| attribute.key == dsl::GROUP_FRAME)
    else {
        return false;
    };
    if attr.span == crate::ast::Span::dummy() {
        return false;
    }
    match &attr.value {
        AttributeValue::Config { algo, options } => {
            let preset_equal =
                algo.eq_ignore_ascii_case(crate::types::attr_constants::group_frame_preset::STRIPS);
            let option_equal = read_str_option(options, "track").is_some_and(|track| {
                track.eq_ignore_ascii_case("equal") || track.eq_ignore_ascii_case("uniform")
            });
            preset_equal || option_equal
        }
        AttributeValue::String(value) => {
            value.eq_ignore_ascii_case(crate::types::attr_constants::group_frame_preset::STRIPS)
        }
        _ => false,
    }
}

/// 是否由 DSL 显式声明了 `group_frame`（任意 arrangement/preset）。
///
/// 供 P1-6 flowchart aspect 自适应判断：仅当用户**未**显式声明时才自动
/// 择优组间轴向，显式声明（如 `c.swimlane-order-process` 的 `axis: horizontal`）
/// 一律尊重、不翻转。
pub(crate) fn has_explicit_group_frame(diagram: &Diagram) -> bool {
    diagram
        .attributes
        .iter()
        .find(|attribute| attribute.key == dsl::GROUP_FRAME)
        .is_some_and(|attr| attr.span != crate::ast::Span::dummy())
}

fn parse_stack_axis(axis: &str) -> Axis {
    match axis.to_ascii_lowercase().as_str() {
        "horizontal" | "h" => Axis::Horizontal,
        _ => Axis::Vertical,
    }
}

/// 将场景短名展开到 `spec`；未知名称返回 `false`。
fn apply_group_frame_preset(name: &str, spec: &mut GroupFrameSpec) -> bool {
    use crate::types::attr_constants::group_frame_preset as preset;
    match name {
        n if n == preset::STRIPS => {
            // 分层条带：水平等宽 + 居中 + 共线边框
            spec.arrangement = GroupArrangement::Stack {
                axis: Axis::Horizontal,
            };
            spec.track_sizing = TrackSizing::Equal;
            spec.cross_align = CrossAlign::Center;
            spec.border_align = BorderAlign::SharedLines;
            true
        }
        n if n == preset::FIT => {
            // 内容贴合：水平堆叠、不拉等宽
            spec.arrangement = GroupArrangement::Stack {
                axis: Axis::Horizontal,
            };
            spec.track_sizing = TrackSizing::Fit;
            true
        }
        n if n == preset::LANES => {
            // 水平泳道：顶对齐 + 较大间距
            spec.arrangement = GroupArrangement::Stack {
                axis: Axis::Horizontal,
            };
            spec.cross_align = CrossAlign::Start;
            spec.gap = 80.0;
            true
        }
        n if n == preset::STAGES => {
            // 纵向阶段：垂直堆叠、内容贴合
            spec.arrangement = GroupArrangement::Stack {
                axis: Axis::Vertical,
            };
            spec.track_sizing = TrackSizing::Fit;
            spec.cross_align = CrossAlign::Center;
            true
        }
        n if n == preset::TILES => {
            // 固定网格：默认 2×2 等宽单元格
            spec.arrangement = GroupArrangement::Matrix {
                rows: Some(2),
                cols: Some(2),
            };
            spec.track_sizing = TrackSizing::Equal;
            spec.gap = 48.0;
            true
        }
        _ => false,
    }
}

/// 从 Config options 读取字符串值（小写归一化）。
fn read_str_option<'a>(options: &'a HashMap<String, AttributeValue>, key: &str) -> Option<&'a str> {
    options.get(key).and_then(|v| v.as_str())
}

/// 从 Config options 读取数值。
fn read_num_option(options: &HashMap<String, AttributeValue>, key: &str) -> Option<f64> {
    match options.get(key)? {
        AttributeValue::Number(n) => Some(*n),
        _ => None,
    }
}

/// P1-4 创新模式 gate：架构图顶层分组二维装箱（shelf packing）。
///
/// 默认**关闭**（opt-in）：仅 `PLOTGRAM_ARCH_PACK=1|on|true` 开启，启用行内
/// shelf 装箱 + 自然宽（Fit）；其余情况（未设/其他值/WASM 无 env 返回 `Err`）
/// 均走旧竖向单列 + 全局等宽。
///
/// 帕累托全量对比结果（showcase 25 架构图）：硬红线 overlap 恒 0 守住、
/// 跨组边长 −8.6%、最差图（k8s-multi-namespace util 2.8%→4.9%）大幅改善；
/// 但 `edge_through_groups` 0→3（单图 c.plotgram-core-mod-deps）、util 平均仅
/// +0.3pp 且 n.d2-cell-tower-network 等小/嵌套图退化。退出判据未干净达成
/// （through 上升 + util 非显著↑），故默认关 gate 兜底，保留代码供后续调优。
pub fn architecture_pack_enabled() -> bool {
    matches!(
        std::env::var("PLOTGRAM_ARCH_PACK").ok().as_deref(),
        Some("1") | Some("on") | Some("true")
    )
}

/// P1-6 创新模式 gate：flowchart 分组图组间摆放 aspect 自适应轴翻转。
///
/// 默认**开启**：`PLOTGRAM_FLOW_ASPECT=0|off|false` 关闭，回退 flowchart
/// 默认竖向堆叠。WASM 无 env → `Err` → 视为开启。仅影响**未显式声明
/// `group_frame`** 的分组图。
///
/// 帕累托 settle（3 个最差样本，红线 overlap/through 全守 0→0）：aspect_ratio
/// 全部大幅趋 1.6（e-commerce 4.29→1.06、ci-cd 3.72→1.14、refund 3.62→1.30）；
/// ci-cd/e-commerce 为干净改善（util↑、边更短或可接受），refund 在 util/
/// edge_length 上退化。综合主目标全改善 + 红线全守，默认 ON；`=0` 作为回退。
pub fn flowchart_aspect_enabled() -> bool {
    !matches!(
        std::env::var("PLOTGRAM_FLOW_ASPECT").ok().as_deref(),
        Some("0") | Some("off") | Some("false")
    )
}

/// architecture 默认：`Stack(H) + Equal + Center + SharedLines`（同级 sibling 条带）。
///
/// 默认等宽条带；显式 `group_frame { track: fit }` 经配置块覆盖退回内容贴合。
/// P1-4：packing 开启时默认 `Fit`（保留装箱自然宽，避免全局等宽把行撑爆）；
/// 显式 `track: equal|uniform` 仍由配置块覆盖回 Equal。
fn resolve_architecture(diagram: &Diagram) -> GroupFrameSpec {
    GroupFrameSpec {
        arrangement: GroupArrangement::Stack {
            axis: Axis::Horizontal,
        },
        track_sizing: if architecture_pack_enabled() {
            TrackSizing::Fit
        } else {
            TrackSizing::Equal
        },
        cross_align: CrossAlign::Center,
        gap: ARCH_GROUP_GAP,
        padding: GroupPadding::architecture_v2(),
        border_align: BorderAlign::SharedLines,
        quantize: resolve_quantize(diagram),
    }
}

/// flowchart / 通用 stack 算法默认（仅由 `group_frame` 覆盖，无旧糖属性）。
fn resolve_stack(diagram: &Diagram) -> GroupFrameSpec {
    GroupFrameSpec {
        arrangement: GroupArrangement::Stack {
            axis: Axis::Vertical,
        },
        track_sizing: TrackSizing::Fit,
        cross_align: CrossAlign::Center,
        gap: FLOWCHART_GROUP_GAP,
        padding: GroupPadding::uniform(SUGIYAMA_GROUP_PADDING, GROUP_LABEL_HEIGHT),
        border_align: BorderAlign::None,
        quantize: resolve_quantize(diagram),
    }
}

/// 解析量化规格：`enabled` = `snap` 属性（默认 true）。
///
/// P0 后 L1 group quantize 仅由 `snap` 控制（像素量化），不再依赖节点对齐开关。
fn resolve_quantize(diagram: &Diagram) -> QuantizeSpec {
    let enabled = diagram_snap_attribute(diagram).unwrap_or(true);
    QuantizeSpec {
        enabled,
        step: DEFAULT_QUANTIZE_STEP,
        quantize_groups: enabled,
    }
}
