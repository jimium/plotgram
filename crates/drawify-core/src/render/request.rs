use crate::ast::{AttributeValue, Diagram, PreparedDiagram};
use crate::layout::LayoutIntentOverlay;
use crate::render::encode::ascii::AsciiExportOptions;
use crate::error::{DrawifyError, Result};
use crate::graphic_style::parse_graphic_style_id;
use crate::render::RenderFormat;
use crate::profile::profile_for;
use crate::types::GraphicStyleId;
use crate::theme::{CompiledRenderContext, ThemeIdResolver, parse_style_sheet_json, is_internal_base};

pub struct RenderRequest<'a> {
    pub diagram: &'a PreparedDiagram,
    pub format: RenderFormat,
    pub explicit_theme_id: Option<&'a str>,
    pub explicit_style_json: Option<&'a str>,
    pub scene_theme_id: Option<&'a str>,
    pub explicit_graphic_style: Option<GraphicStyleId>,
    pub scene_graphic_style: Option<GraphicStyleId>,
    pub dark_mode: bool,
    /// 是否在 SVG 右下角输出 "powered by drawify" 署名（默认开启）。
    pub attribution: bool,
    /// 强制省略画布背景 rect（不受 theme canvas.background 影响）。
    pub transparent_background: bool,
    /// 是否启用 semantic → icon 推断（默认开启）。
    pub semantic_inference: bool,
    pub ascii_options: AsciiExportOptions,
    /// 布局意图叠加层（可选）。
    ///
    /// 透传至 `compute_layout_with_plan_and_overlay`，由布局算法与几何微调阶段消费。
    /// 为 `None` 时布局行为与无意图完全一致。
    pub layout_overlay: Option<&'a LayoutIntentOverlay>,
}

impl<'a> RenderRequest<'a> {
    pub fn new(diagram: &'a PreparedDiagram, format: RenderFormat) -> Self {
        Self {
            diagram,
            format,
            explicit_theme_id: None,
            explicit_style_json: None,
            scene_theme_id: None,
            explicit_graphic_style: None,
            scene_graphic_style: None,
            dark_mode: false,
            attribution: true,
            transparent_background: false,
            semantic_inference: true,
            ascii_options: AsciiExportOptions::default(),
            layout_overlay: None,
        }
    }

    pub fn user_theme_id(&self) -> Option<&'a str> {
        self.explicit_theme_id
    }

    pub fn scene_theme_id(&self) -> Option<&'a str> {
        self.scene_theme_id
    }

    pub fn resolve_theme_id(&self) -> &'a str {
        ThemeIdResolver::new(&self.diagram.diagram_type)
            .explicit(self.explicit_theme_id)
            .scene(self.scene_theme_id)
            .dark_mode(self.dark_mode)
            .resolve()
    }

    pub fn resolve_graphic_style(&self) -> GraphicStyleId {
        self.explicit_graphic_style
            .or(self.scene_graphic_style)
            .or_else(|| graphic_style_from_diagram(self.diagram))
            .unwrap_or_else(|| profile_for(&self.diagram.diagram_type).default_graphic_style)
    }

    pub fn resolve_context(&self) -> Result<CompiledRenderContext> {
        let compiled = if let Some(style_json) = self.explicit_style_json {
            let sheet = parse_style_sheet_json(style_json)?;
            crate::theme::compile_theme(sheet)
                .map_err(|e| DrawifyError::Style(format!("compile custom theme: {e}")))?
        } else {
            let theme_id = self.resolve_theme_id();
            // 拒绝内部基座（§10.2/§13.9）
            if is_internal_base(theme_id) {
                return Err(DrawifyError::Style(format!(
                    "theme '{theme_id}' is an internal base and cannot be used directly"
                )));
            }
            crate::theme::compiled_builtin_theme(theme_id)
                .ok_or_else(|| DrawifyError::Style(format!("unknown builtin theme '{theme_id}'")))?
        };
        Ok(CompiledRenderContext {
            compiled,
            graphic_style: self.resolve_graphic_style(),
            icon_resolve: crate::icons::ResolveOptions {
                semantic_inference: self.semantic_inference,
            },
        })
    }
}

fn graphic_style_from_diagram(diagram: &Diagram) -> Option<GraphicStyleId> {
    for attr in &diagram.attributes {
        if attr.key == "render_style" || attr.key == "render-style" {
            let value = match &attr.value {
                AttributeValue::String(s) => s.as_str(),
                _ => continue,
            };
            return parse_graphic_style_id(value);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::DiagramType;
    use crate::ast::{AttributeValue, DiagramAttribute, PreparedDiagram, SourceInfo, TextValue};

    fn sample_prepared(diagram_type: DiagramType) -> PreparedDiagram {
        PreparedDiagram::new(Diagram::new(
            diagram_type,
            SourceInfo {
                file: None,
                line_count: 1,
            },
        ))
    }

    #[test]
    fn explicit_theme_id_has_highest_priority() {
        let prepared = sample_prepared(DiagramType::Architecture);
        let request = RenderRequest {
            diagram: &prepared,
            format: RenderFormat::Svg,
            explicit_theme_id: Some("common.presentation"),
            explicit_style_json: None,
            scene_theme_id: Some("common.blueprint"),
            explicit_graphic_style: None,
            scene_graphic_style: None,
            dark_mode: true,
            attribution: true,
            transparent_background: false,
            semantic_inference: true,
            ascii_options: AsciiExportOptions::default(),
            layout_overlay: None,
        };

        assert_eq!(request.resolve_theme_id(), "common.presentation");
    }

    #[test]
    fn dark_mode_uses_profile_dark_theme_when_not_overridden() {
        let prepared = sample_prepared(DiagramType::Flowchart);
        let request = RenderRequest {
            diagram: &prepared,
            format: RenderFormat::Svg,
            explicit_theme_id: None,
            explicit_style_json: None,
            scene_theme_id: None,
            explicit_graphic_style: None,
            scene_graphic_style: None,
            dark_mode: true,
            attribution: true,
            transparent_background: false,
            semantic_inference: true,
            ascii_options: AsciiExportOptions::default(),
            layout_overlay: None,
        };

        assert_eq!(request.resolve_theme_id(), "common.clean-dark");
    }

    #[test]
    fn explicit_style_json_resolves_without_builtin_lookup() {
        let prepared = sample_prepared(DiagramType::Flowchart);
        let request = RenderRequest {
            diagram: &prepared,
            format: RenderFormat::Svg,
            explicit_theme_id: None,
            explicit_style_json: Some(
                r##"{
                    "version":"0.2",
                    "id":"custom.inline",
                    "name":"Inline",
                    "tokens":{"colors":{"canvas":"#ffffff","text":"#333333"}},
                    "defaults":{
                        "canvas":{"background":"#ffffff"},
                        "title":{"fill":"#333333"},
                        "node":{"fill":"#eeeeee","stroke":"#999999"},
                        "edge":{"stroke":"#999999"},
                        "group":{"fill":"#f5f5f5","stroke":"#cccccc"}
                    },
                    "diagrams":{}
                }"##,
            ),
            scene_theme_id: None,
            explicit_graphic_style: None,
            scene_graphic_style: None,
            dark_mode: false,
            attribution: true,
            transparent_background: false,
            semantic_inference: true,
            ascii_options: AsciiExportOptions::default(),
            layout_overlay: None,
        };

        let context = request.resolve_context().unwrap();
        assert_eq!(context.compiled.id, "custom.inline");
        assert_eq!(context.graphic_style, GraphicStyleId::Standard);
    }

    #[test]
    fn resolve_graphic_style_from_diagram_attribute() {
        let mut diagram = Diagram::new(
            DiagramType::Flowchart,
            SourceInfo {
                file: None,
                line_count: 1,
            },
        );
        diagram.attributes.push(DiagramAttribute {
            key: "render-style".to_string(),
            value: AttributeValue::String(TextValue::quoted("spatial-clarity")),
            span: crate::ast::Span::dummy(),
        });

        let prepared = PreparedDiagram::new(diagram);
        let request = RenderRequest {
            diagram: &prepared,
            format: RenderFormat::Svg,
            explicit_theme_id: None,
            explicit_style_json: None,
            scene_theme_id: None,
            explicit_graphic_style: None,
            scene_graphic_style: None,
            dark_mode: false,
            attribution: true,
            transparent_background: false,
            semantic_inference: true,
            ascii_options: AsciiExportOptions::default(),
            layout_overlay: None,
        };

        assert_eq!(
            request.resolve_graphic_style(),
            GraphicStyleId::SpatialClarity
        );
    }
}
