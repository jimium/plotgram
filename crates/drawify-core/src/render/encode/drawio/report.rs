//! draw.io 导出选项、降级级别与导出报告。

use crate::types::DiagramType;
use serde::Serialize;

/// draw.io 导出选项。
#[derive(Debug, Clone)]
pub struct DrawioExportOptions {
    /// 允许不支持的图表类型（sequence/er）
    pub allow_unsupported_diagram_types: bool,
    /// 不支持类型的降级方式
    pub fallback: DrawioFallback,
    /// 折线路径最多导出的中间拐点数（不含 source/target 端点）；超出时保留首尾拐点并降级 L1
    pub max_edge_waypoints: u8,
    /// 是否包含 drawify 元数据（drawifyEntityId / drawifyRelationIndex / drawifyDegrade 等）
    pub include_export_metadata: bool,
    /// 画布外留白
    pub page_padding: f64,
    /// 是否输出压缩格式 .drawio（deflate + base64，需要 `compressed-drawio` feature）
    pub compressed: bool,
    /// 是否将节点图标嵌入为 draw.io image shape（spec §6.4 L1）；false 时直接 L2
    pub embed_icons: bool,
}

impl Default for DrawioExportOptions {
    fn default() -> Self {
        Self {
            allow_unsupported_diagram_types: false,
            fallback: DrawioFallback::Error,
            max_edge_waypoints: 2,
            include_export_metadata: true,
            page_padding: 20.0,
            compressed: false,
            embed_icons: true,
        }
    }
}

/// 不支持类型的降级策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrawioFallback {
    /// 拒绝导出，返回错误
    Error,
    /// 嵌入整张 SVG image（不可编辑）
    EmbeddedSvg,
}

// ─── 降级级别 ────────────────────────────────────────────────

/// 降级级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum DegradeTier {
    /// 完整结构化映射
    L0,
    /// 形状/样式近似
    L1,
    /// 部分语义丢失
    L2,
    /// 元素级跳过
    L3,
    /// 整图 SVG 嵌入
    F,
}

impl DegradeTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            DegradeTier::L0 => "L0",
            DegradeTier::L1 => "L1",
            DegradeTier::L2 => "L2",
            DegradeTier::L3 => "L3",
            DegradeTier::F => "F",
        }
    }
}

// ─── 导出报告 ────────────────────────────────────────────────

/// 单条导出警告。
#[derive(Debug, Clone, Serialize)]
pub struct ExportWarning {
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entity_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_index: Option<usize>,
    pub tier: DegradeTier,
    pub message: String,
}

/// 导出统计。
#[derive(Debug, Clone, Default, Serialize)]
pub struct ExportStats {
    pub nodes: usize,
    pub edges: usize,
    pub groups: usize,
    pub l0: usize,
    pub l1: usize,
    pub l2: usize,
    pub l3: usize,
}

/// 导出报告。
#[derive(Debug, Clone, Serialize)]
pub struct ExportReport {
    pub format: String,
    pub export_version: String,
    pub diagram_type: DiagramType,
    pub global_degrade: DegradeTier,
    pub warnings: Vec<ExportWarning>,
    pub stats: ExportStats,
}

impl ExportReport {
    pub fn new(diagram_type: &DiagramType) -> Self {
        Self {
            format: "drawio".to_string(),
            export_version: "0.1".to_string(),
            diagram_type: diagram_type.clone(),
            global_degrade: DegradeTier::L0,
            warnings: Vec::new(),
            stats: ExportStats::default(),
        }
    }
}
