//! Hierarchical Profile（23 §7.5.4 / doc 21 §5.2）。
//!
//! 内核只认本结构；`DiagramType` 不得渗入 solve / ink / channel。

/// 组间策略：弱堆叠（flowchart）vs 强宏观分层（architecture）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupPolicy {
    /// 组间拓扑堆叠（原 `group_divide`）。
    Weak,
    /// 超级图 macro rank（原 `two_phase` Phase B）。
    StrongMacro,
}

/// 同级组框宽度策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupSizing {
    Fit,
    Equal,
}

/// 宏观行 / 组间对齐。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupAlign {
    Start,
    Center,
    End,
}

/// 疏密档（映射层间距 / pair gap 倍率）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Density {
    Compact,
    Standard,
    Spacious,
}

impl Density {
    /// 相对 Standard 的间距倍率。
    pub fn gap_scale(self) -> f64 {
        match self {
            Self::Compact => 0.75,
            Self::Standard => 1.0,
            Self::Spacious => 1.35,
        }
    }
}

/// Hierarchical 方言的可调参数。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HierarchicalProfile {
    pub group_policy: GroupPolicy,
    pub group_sizing: GroupSizing,
    pub group_align: GroupAlign,
    pub density: Density,
    /// architecture：hub 居中 + client 共线；flowchart 关。
    pub hub_client_align: bool,
    /// 选用哪套 Sugiyama preset（不暴露 DiagramType）。
    pub preset: HierarchicalPreset,
}

/// 分层内核 preset 选择（Dialect 编译结果，非图种）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HierarchicalPreset {
    Flowchart,
    Architecture,
    State,
}

impl HierarchicalProfile {
    pub fn flowchart_default() -> Self {
        Self {
            group_policy: GroupPolicy::Weak,
            group_sizing: GroupSizing::Fit,
            group_align: GroupAlign::Center,
            density: Density::Standard,
            hub_client_align: false,
            preset: HierarchicalPreset::Flowchart,
        }
    }

    pub fn architecture_default() -> Self {
        Self {
            group_policy: GroupPolicy::StrongMacro,
            group_sizing: GroupSizing::Equal,
            group_align: GroupAlign::Start,
            density: Density::Standard,
            hub_client_align: true,
            preset: HierarchicalPreset::Architecture,
        }
    }

    pub fn state_default() -> Self {
        Self {
            group_policy: GroupPolicy::Weak,
            group_sizing: GroupSizing::Fit,
            group_align: GroupAlign::Center,
            density: Density::Standard,
            hub_client_align: false,
            preset: HierarchicalPreset::State,
        }
    }
}
