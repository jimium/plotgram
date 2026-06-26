//! Drawify → draw.io 格式导出器
//!
//! 将 `ExportScene` 编码为 diagrams.net / draw.io 原生 `.drawio`（mxGraphModel XML）格式。
//!
//! 说明见同目录 [`README.md`](README.md)。

mod compress;
mod encoder;
mod icon;
mod report;
mod routing;
mod style;

use crate::error::Result;
use crate::render::scene::ExportScene;
use crate::render::{FormatEncoder, RenderFormat, RenderOutput};

pub use report::{DegradeTier, DrawioExportOptions, DrawioFallback, ExportReport, ExportStats, ExportWarning};

pub struct DrawioRenderer;

impl FormatEncoder for DrawioRenderer {
    fn format(&self) -> RenderFormat {
        RenderFormat::Drawio
    }

    fn name(&self) -> &str {
        "drawio"
    }

    fn description(&self) -> &str {
        "diagrams.net / draw.io 原生格式 - 可编辑的结构化图导出"
    }

    fn encode_scene(&self, scene: &ExportScene<'_>) -> Result<RenderOutput> {
        let options = DrawioExportOptions::default();
        let (xml, _report) = encode_scene_inner(scene, &options)?;
        Ok(RenderOutput::Text(xml))
    }

    fn encode_scene_with_report(
        &self,
        scene: &ExportScene<'_>,
    ) -> Result<(RenderOutput, Option<ExportReport>)> {
        let options = DrawioExportOptions::default();
        let (xml, report) = encode_scene_inner(scene, &options)?;
        Ok((RenderOutput::Text(xml), Some(report)))
    }

    fn file_extension(&self) -> &str {
        "drawio"
    }
}

/// 将 `ExportScene` 编码为 draw.io XML，同时返回导出报告。
pub fn encode_scene_inner(
    scene: &ExportScene<'_>,
    options: &DrawioExportOptions,
) -> Result<(String, ExportReport)> {
    let enc = encoder::DrawioEncoder::new(scene, options);
    enc.encode()
}
