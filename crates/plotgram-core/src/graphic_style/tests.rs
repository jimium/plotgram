//! graphic_style 模块的单元测试。
//!
//! 从 `graphic_style/mod.rs` 迁出，保持主文件聚焦实现。

use crate::graphic_style::{parse_graphic_style_id, painter_for};
use crate::types::GraphicStyleId;
use crate::render::visual::{EdgeStyle, NodeShape, NodeStyle};
use crate::graphic_style::common;

#[test]
fn parse_excalidraw_canonical_id() {
    assert_eq!(parse_graphic_style_id("excalidraw"), Some(GraphicStyleId::Excalidraw));
}

#[test]
fn excalidraw_painter_decorates_structured_style_fields() {
    let mut node_style = NodeStyle::default();
    let mut edge_style = EdgeStyle::default();

    let painter = painter_for(GraphicStyleId::Excalidraw);
    painter.decorate_node_style(&mut node_style);
    painter.decorate_edge_style(&mut edge_style);

    assert_eq!(painter.id(), GraphicStyleId::Excalidraw);
    assert_eq!(node_style.stroke_linecap.as_deref(), Some("round"));
    assert_eq!(node_style.stroke_linejoin.as_deref(), Some("round"));
    assert!(node_style.hand_drawn);
    assert!(edge_style.hand_drawn);
}

#[test]
fn excalidraw_painter_renders_all_builtin_node_shapes() {
    let mut style = NodeStyle::default();
    let painter = painter_for(GraphicStyleId::Excalidraw);
    painter.decorate_node_style(&mut style);

    let shapes = [
        NodeShape::Rect,
        NodeShape::RoundedRect,
        NodeShape::Circle,
        NodeShape::Diamond,
        NodeShape::Cylinder,
        NodeShape::Hexagon,
        NodeShape::Person,
        NodeShape::Stadium,
        NodeShape::Parallelogram,
        NodeShape::Document,
        NodeShape::Cloud,
        NodeShape::Subprocess,
    ];

    for shape in shapes {
        let svg = painter
            .render_node_shape(&shape, 10.0, 20.0, 120.0, 56.0, &style)
            .unwrap();
        assert!(svg.contains("<g"));
        assert!(svg.contains("data-graphic-style=\"excalidraw\""));
        assert!(svg.contains("stroke=\""));
    }

    let diamond_svg = painter
        .render_node_shape(&NodeShape::Diamond, 10.0, 20.0, 120.0, 56.0, &style)
        .unwrap();
    assert!(
        diamond_svg.contains("<clipPath"),
        "excalidraw fill should be clipped to shape path"
    );
    assert!(
        diamond_svg.contains("<circle"),
        "excalidraw fill should use dot pattern"
    );
}

#[test]
fn excalidraw_painter_roughens_edge_paths() {
    let mut style = EdgeStyle::default();
    let painter = painter_for(GraphicStyleId::Excalidraw);
    painter.decorate_edge_style(&mut style);

    let svg = painter
        .render_edge_path(
            "M 10 10 L 60 10 Q 90 10 90 40",
            "#333333",
            &style,
            "url(#arrow-active)",
            "",
        )
        .unwrap();

    assert!(svg.contains("C "));
    assert!(svg.contains("marker-end=\"url(#arrow-active)\""));
    assert!(svg.contains("data-graphic-style=\"excalidraw\""));
}

#[test]
fn excalidraw_dot_fill_produces_clipped_circles() {
    use crate::graphic_style::excalidraw;
    let style = NodeStyle::default();
    let bbox = (0.0, 0.0, 100.0, 50.0);
    let clip = r#"<path d="M 0 0 L 100 0 L 100 50 L 0 50 Z"/>"#;
    let dots = excalidraw::excalidraw_dot_fill(clip, &bbox, &style);
    assert!(dots.contains("<clipPath"));
    assert!(dots.contains("<circle"));
    assert!(dots.contains("opacity=\"0.38\""));
}

#[test]
fn clip_line_to_rect_basic() {
    use crate::graphic_style::common::clip_line_to_rect;
    let result = clip_line_to_rect(10.0, 10.0, 50.0, 40.0, 0.0, 0.0, 100.0, 100.0);
    assert!(result.is_some());
    let (x1, y1, x2, y2) = result.unwrap();
    assert!((x1 - 10.0).abs() < 0.01);
    assert!((y1 - 10.0).abs() < 0.01);
    assert!((x2 - 50.0).abs() < 0.01);
    assert!((y2 - 40.0).abs() < 0.01);

    assert!(clip_line_to_rect(-50.0, -50.0, -20.0, -20.0, 0.0, 0.0, 100.0, 100.0).is_none());
}

// --- New style tests ---

#[test]
fn parse_graphic_style_canonical_ids() {
    assert_eq!(parse_graphic_style_id("standard"), Some(GraphicStyleId::Standard));
    assert_eq!(parse_graphic_style_id("cross-hatch"), Some(GraphicStyleId::CrossHatch));
    assert_eq!(parse_graphic_style_id("blueprint"), Some(GraphicStyleId::Blueprint));
    assert_eq!(
        parse_graphic_style_id("spatial-clarity"),
        Some(GraphicStyleId::SpatialClarity)
    );
    assert_eq!(parse_graphic_style_id("neon-glow"), Some(GraphicStyleId::NeonGlow));
    assert_eq!(parse_graphic_style_id("stipple"), Some(GraphicStyleId::Stipple));
}

#[test]
fn parse_graphic_style_rejects_aliases() {
    for alias in [
        "hand-drawn",
        "handdrawn",
        "sketch",
        "crosshatch",
        "spatial",
        "clarity",
        "system",
        "neon",
        "glow",
        "dot-fill",
        "pointillism",
        "calligraphic",
        "cartoon",
        "dotted",
    ] {
        assert_eq!(parse_graphic_style_id(alias), None, "alias should be rejected: {alias}");
    }
}

#[test]
fn new_styles_render_all_builtin_node_shapes() {
    let styles = [
        GraphicStyleId::CrossHatch,
        GraphicStyleId::Blueprint,
        GraphicStyleId::SpatialClarity,
        GraphicStyleId::Stipple,
    ];

    let shapes = [
        NodeShape::Rect,
        NodeShape::RoundedRect,
        NodeShape::Circle,
        NodeShape::Diamond,
        NodeShape::Cylinder,
        NodeShape::Hexagon,
        NodeShape::Person,
        NodeShape::Stadium,
        NodeShape::Parallelogram,
        NodeShape::Document,
        NodeShape::Cloud,
        NodeShape::Subprocess,
    ];

    for style_id in styles {
        let mut style = NodeStyle::default();
        let painter = painter_for(style_id);
        painter.decorate_node_style(&mut style);

        for shape in &shapes {
            let svg = painter
                .render_node_shape(shape, 10.0, 20.0, 120.0, 56.0, &style)
                .unwrap_or_else(|| panic!("{style_id:?} should render {shape:?}"));
            assert!(svg.contains("<g"), "missing <g> for {style_id:?} / {shape:?}");
            assert!(svg.contains("stroke=\""), "missing stroke for {style_id:?} / {shape:?}");
        }
    }
}

#[test]
fn new_styles_render_edge_paths() {
    let styles = [
        GraphicStyleId::CrossHatch,
        GraphicStyleId::Blueprint,
        GraphicStyleId::SpatialClarity,
    ];

    for style_id in styles {
        let mut style = EdgeStyle::default();
        let painter = painter_for(style_id);
        painter.decorate_edge_style(&mut style);

        let svg = painter
            .render_edge_path(
                "M 10 10 L 60 10 Q 90 10 90 40",
                "#333333",
                &style,
                "url(#arrow-active)",
                "",
            )
            .unwrap_or_else(|| panic!("{style_id:?} should render edge path"));

        assert!(svg.contains("marker-end=\"url(#arrow-active)\""), "missing marker for {style_id:?}");
    }
}

#[test]
fn cross_hatch_has_denser_fill_than_excalidraw() {
    use crate::graphic_style::excalidraw;
    let bbox = (0.0, 0.0, 100.0, 50.0);
    let style = NodeStyle::default();
    let clip = r#"<path d="M 0 0 L 100 0 L 100 50 L 0 50 Z"/>"#;

    let excalidraw_svg = excalidraw::excalidraw_dot_fill(clip, &bbox, &style);
    let cross_hatch_svg = common::clipped_dot_fill(
        clip,
        "ch",
        &bbox,
        &style.fill,
        6.0,
        1.0,
        0.42,
    );

    let excalidraw_dots = excalidraw_svg.matches("<circle").count();
    let cross_hatch_dots = cross_hatch_svg.matches("<circle").count();
    assert!(
        cross_hatch_dots > excalidraw_dots,
        "cross-hatch should have more dots than excalidraw"
    );
}

#[test]
fn blueprint_has_miter_join_and_semi_transparent_fill() {
    let mut style = NodeStyle::default();
    let painter = painter_for(GraphicStyleId::Blueprint);
    painter.decorate_node_style(&mut style);

    assert_eq!(style.stroke_linejoin.as_deref(), Some("miter"));

    let svg = painter.render_node_shape(&NodeShape::Rect, 10.0, 20.0, 120.0, 56.0, &style).unwrap();
    assert!(svg.contains("fill-opacity=\"0.15\""));
}

#[test]
fn common_hachure_fill_produces_lines() {
    let bbox = (0.0, 0.0, 100.0, 50.0);
    let hachure = common::hachure_fill(&bbox, "#333", 8.0, -std::f64::consts::PI / 6.0, 0.7, 0.18, 0.4);
    assert!(hachure.contains("M "));
    assert!(hachure.contains("opacity=\"0.18\""));
}

#[test]
fn common_dot_fill_produces_circles() {
    let bbox = (0.0, 0.0, 100.0, 50.0);
    let dots = common::dot_fill(&bbox, "#333", 10.0, 1.0, 0.25);
    assert!(dots.contains("<circle"));
    assert!(dots.contains("opacity=\"0.25\""));
}

#[test]
fn spatial_clarity_painter_decorates_without_hand_drawn() {
    let mut node_style = NodeStyle::default();
    let mut edge_style = EdgeStyle::default();
    let painter = painter_for(GraphicStyleId::SpatialClarity);
    painter.decorate_node_style(&mut node_style);
    painter.decorate_edge_style(&mut edge_style);

    assert_eq!(painter.id(), GraphicStyleId::SpatialClarity);
    assert!(!node_style.hand_drawn);
    assert!(!edge_style.hand_drawn);
    assert_eq!(node_style.stroke_width, 1.0);
    assert_eq!(edge_style.stroke_width, 1.25);
    assert_eq!(node_style.stroke_linecap.as_deref(), Some("round"));
}

#[test]
fn spatial_clarity_renders_shadow_and_soft_stroke() {
    let mut style = NodeStyle::default();
    let painter = painter_for(GraphicStyleId::SpatialClarity);
    painter.decorate_node_style(&mut style);

    let svg = painter
        .render_node_shape(&NodeShape::RoundedRect, 10.0, 20.0, 120.0, 56.0, &style)
        .unwrap();

    assert!(svg.contains("data-graphic-style=\"spatial-clarity\""));
    assert!(svg.contains("filter=\"url(#sc-shadow)\""));
    assert!(svg.contains("fill-opacity=\"0.96\""));
    assert!(svg.contains("stroke-opacity=\"0.1\""));
}

#[test]
fn spatial_clarity_shared_defs_include_shadow_filter() {
    let painter = painter_for(GraphicStyleId::SpatialClarity);
    let defs = painter.shared_svg_defs().unwrap();
    assert!(defs.contains("id=\"sc-shadow\""));
    assert!(defs.contains("feDropShadow"));
}

#[test]
fn spatial_clarity_edge_uses_rounded_corners() {
    let mut style = EdgeStyle::default();
    let painter = painter_for(GraphicStyleId::SpatialClarity);
    painter.decorate_edge_style(&mut style);

    let svg = painter
        .render_edge_path(
            "M 10 10 L 60 10 L 60 40",
            "#8E8E93",
            &style,
            "url(#arrow-active)",
            "",
        )
        .unwrap();

    assert!(svg.contains(" Q "));
    assert!(svg.contains("stroke-opacity=\"0.55\""));
    assert!(svg.contains("data-graphic-style=\"spatial-clarity\""));
}

// --- Neon Glow tests ---

#[test]
fn neon_glow_painter_decorates_without_hand_drawn() {
    let mut node_style = NodeStyle::default();
    let mut edge_style = EdgeStyle::default();
    let painter = painter_for(GraphicStyleId::NeonGlow);
    painter.decorate_node_style(&mut node_style);
    painter.decorate_edge_style(&mut edge_style);

    assert_eq!(painter.id(), GraphicStyleId::NeonGlow);
    assert!(!node_style.hand_drawn);
    assert!(!edge_style.hand_drawn);
    assert_eq!(node_style.stroke_width, 2.0);
    assert_eq!(edge_style.stroke_width, 2.0);
    assert_eq!(node_style.stroke_linecap.as_deref(), Some("round"));
}

#[test]
fn neon_glow_shared_defs_include_glow_filter() {
    let painter = painter_for(GraphicStyleId::NeonGlow);
    let defs = painter.shared_svg_defs().unwrap();
    assert!(defs.contains("id=\"ng-glow\""));
    assert!(defs.contains("id=\"ng-glow-soft\""));
    assert!(defs.contains("feGaussianBlur"));
    assert!(defs.contains("feMerge"));
}

#[test]
fn neon_glow_renders_all_builtin_node_shapes() {
    let mut style = NodeStyle::default();
    let painter = painter_for(GraphicStyleId::NeonGlow);
    painter.decorate_node_style(&mut style);

    let shapes = [
        NodeShape::Rect,
        NodeShape::RoundedRect,
        NodeShape::Circle,
        NodeShape::Diamond,
        NodeShape::Cylinder,
        NodeShape::Hexagon,
        NodeShape::Person,
        NodeShape::Stadium,
        NodeShape::Parallelogram,
        NodeShape::Document,
        NodeShape::Cloud,
        NodeShape::Subprocess,
    ];

    for shape in shapes {
        let svg = painter
            .render_node_shape(&shape, 10.0, 20.0, 120.0, 56.0, &style)
            .unwrap();
        assert!(svg.contains("<g"), "missing <g> for {shape:?}");
        assert!(svg.contains("data-graphic-style=\"neon-glow\""), "missing neon-glow attr for {shape:?}");
        assert!(svg.contains("filter=\"url(#ng-glow)\""), "missing glow filter for {shape:?}");
        assert!(svg.contains("stroke=\""), "missing stroke for {shape:?}");
    }
}

#[test]
fn neon_glow_renders_edge_path_with_glow() {
    let mut style = EdgeStyle::default();
    let painter = painter_for(GraphicStyleId::NeonGlow);
    painter.decorate_edge_style(&mut style);

    let svg = painter
        .render_edge_path(
            "M 10 10 L 60 10 Q 90 10 90 40",
            "#00FFCC",
            &style,
            "url(#arrow-active)",
            "",
        )
        .unwrap();

    assert!(svg.contains("filter=\"url(#ng-glow)\""));
    assert!(svg.contains("marker-end=\"url(#arrow-active)\""));
    assert!(svg.contains("data-graphic-style=\"neon-glow\""));
    assert!(svg.contains("stroke-opacity=\"0.55\""));
}

#[test]
fn neon_glow_fill_is_very_low_opacity() {
    let mut style = NodeStyle::default();
    let painter = painter_for(GraphicStyleId::NeonGlow);
    painter.decorate_node_style(&mut style);

    let svg = painter
        .render_node_shape(&NodeShape::Rect, 10.0, 20.0, 120.0, 56.0, &style)
        .unwrap();

    assert!(svg.contains("fill-opacity=\"0.08\""));
}

// --- Stipple tests ---

#[test]
fn stipple_painter_uses_hand_drawn_and_rough_outline() {
    let mut node_style = NodeStyle::default();
    let mut edge_style = EdgeStyle::default();
    let painter = painter_for(GraphicStyleId::Stipple);
    painter.decorate_node_style(&mut node_style);
    painter.decorate_edge_style(&mut edge_style);

    assert_eq!(painter.id(), GraphicStyleId::Stipple);
    assert!(node_style.hand_drawn);
    assert!(edge_style.hand_drawn);
    assert_eq!(node_style.stroke_width, 2.0);
    assert_eq!(edge_style.stroke_width, 2.0);
}

#[test]
fn stipple_fill_uses_dot_circles() {
    let mut style = NodeStyle::default();
    let painter = painter_for(GraphicStyleId::Stipple);
    painter.decorate_node_style(&mut style);

    let svg = painter
        .render_node_shape(&NodeShape::Rect, 10.0, 20.0, 120.0, 56.0, &style)
        .unwrap();

    // Dot fill produces <circle> elements
    assert!(svg.contains("<circle"), "stipple should contain dot fill circles");
    // Outline should use rough polylines (cubic bezier)
    assert!(svg.contains(" C "), "stipple should use rough bezier outlines");
    // Should have the stipple data attr
    assert!(svg.contains("data-graphic-style=\"stipple\""));
}

#[test]
fn stipple_edge_uses_smooth_rounded_corners() {
    let mut style = EdgeStyle::default();
    let painter = painter_for(GraphicStyleId::Stipple);
    painter.decorate_edge_style(&mut style);

    let svg = painter
        .render_edge_path(
            "M 10 10 L 60 10 Q 90 10 90 40",
            "#333333",
            &style,
            "url(#arrow-active)",
            "",
        )
        .unwrap();

    // smooth_polyline_path produces Q commands for rounded corners
    assert!(svg.contains(" Q "), "stipple edges should use rounded corners");
    assert!(svg.contains("marker-end=\"url(#arrow-active)\""));
    assert!(svg.contains("data-graphic-style=\"stipple\""));
}

#[test]
fn stipple_has_no_shared_defs() {
    let painter = painter_for(GraphicStyleId::Stipple);
    assert!(painter.shared_svg_defs().is_none());
}
