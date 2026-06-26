//! 内置节点形状 → SVG 几何。

use super::super::visual::{NodeShape, NodeStyle};
use crate::graphic_style::common::{
    cloud_points, document_path, parallelogram_points, subprocess_inset,
};
use crate::render::CompiledRenderContext;

impl NodeShape {
    pub fn render_with_context(
        &self,
        x: f64,
        y: f64,
        width: f64,
        height: f64,
        style: &NodeStyle,
        context: &CompiledRenderContext,
    ) -> String {
        if let Some(svg) = context
            .graphic_painter()
            .render_node_shape(self, x, y, width, height, style)
        {
            return svg;
        }

        self.render(x, y, width, height, style)
    }

    /// 获取 SVG 形状的路径或元素。
    pub fn render(&self, x: f64, y: f64, width: f64, height: f64, style: &NodeStyle) -> String {
        let extra = node_svg_attrs(style, x + width / 2.0, y + height / 2.0);
        match self {
            Self::Rect => {
                let rx = style.corner_radius(self, width, height);
                format!(
                    r##"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width}" {extra}/>"##,
                    x = x, y = y, w = width, h = height, rx = rx, fill = style.fill, stroke = style.stroke, stroke_width = style.stroke_width, extra = extra
                )
            }
            Self::RoundedRect => {
                let rx = style.corner_radius(self, width, height);
                format!(
                    r##"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width}" {extra}/>"##,
                    x = x, y = y, w = width, h = height, rx = rx, fill = style.fill, stroke = style.stroke, stroke_width = style.stroke_width, extra = extra
                )
            }
            Self::Circle => {
                let r = width.min(height) / 2.0;
                let cx = x + width / 2.0;
                let cy = y + height / 2.0;
                format!(
                    r##"<circle cx="{cx}" cy="{cy}" r="{r}" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width}" {extra}/>"##,
                    cx = cx, cy = cy, r = r, fill = style.fill, stroke = style.stroke, stroke_width = style.stroke_width, extra = extra
                )
            }
            Self::Diamond => {
                let cx = x + width / 2.0;
                let cy = y + height / 2.0;
                let points = format!(
                    "{cx},{y} {x_w},{cy} {cx},{y_h} {x},{cy}",
                    cx = cx, cy = cy,
                    x = x, y = y,
                    x_w = x + width, y_h = y + height
                );
                format!(
                    r##"<polygon points="{points}" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width}" {extra}/>"##,
                    points = points, fill = style.fill, stroke = style.stroke, stroke_width = style.stroke_width, extra = extra
                )
            }
            Self::Cylinder => {
                let ry = 8.0;
                format!(
                    r##"<path d="M {x} {y_ry} A {w} {ry} 0 0 1 {x_w} {y_ry} L {x_w} {y_h_ry} A {w} {ry} 0 0 1 {x} {y_h_ry} Z" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width}" {extra}/>
<ellipse cx="{cx}" cy="{y_ry}" rx="{w}" ry="{ry}" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width}" {extra}/>"##,
                    x = x, y_ry = y + ry, x_w = x + width, y_h_ry = y + height - ry,
                    cx = x + width / 2.0, w = width / 2.0, ry = ry,
                    fill = style.fill, stroke = style.stroke, stroke_width = style.stroke_width, extra = extra
                )
            }
            Self::Hexagon => {
                let h3 = height / 3.0;
                let w4 = width / 4.0;
                let points = format!(
                    "{x_w4} {y} {x3w4} {y} {x_w} {y_h3} {x3w4} {y_h23} {x_w4} {y_h23} {x} {y_h3}",
                    x = x, y = y, x_w = x + width, y_h3 = y + h3, y_h23 = y + height - h3,
                    x_w4 = x + w4, x3w4 = x + width - w4
                );
                format!(
                    r##"<polygon points="{points}" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width}" {extra}/>"##,
                    points = points, fill = style.fill, stroke = style.stroke, stroke_width = style.stroke_width, extra = extra
                )
            }
            Self::Person => {
                let head_r = width.min(height) / 4.0;
                let head_cx = x + width / 2.0;
                let head_cy = y + head_r;
                format!(
                    r##"<circle cx="{cx}" cy="{cy}" r="{r}" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width}" {extra}/>
<path d="M {x} {y_h} L {cx} {y_sh} L {x_w} {y_h} Z" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width}" {extra}/>"##,
                    cx = head_cx, cy = head_cy, r = head_r,
                    x = x, y_h = y + height, x_w = x + width, y_sh = y + height / 3.0,
                    fill = style.fill, stroke = style.stroke, stroke_width = style.stroke_width, extra = extra
                )
            }
            Self::Stadium => {
                let rx = style.corner_radius(self, width, height);
                format!(
                    r##"<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width}" {extra}/>"##,
                    x = x, y = y, w = width, h = height, rx = rx,
                    fill = style.fill, stroke = style.stroke, stroke_width = style.stroke_width, extra = extra
                )
            }
            Self::Parallelogram => {
                let points = polygon_points(&parallelogram_points(x, y, width, height));
                format!(
                    r##"<polygon points="{points}" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width}" {extra}/>"##,
                    points = points, fill = style.fill, stroke = style.stroke, stroke_width = style.stroke_width, extra = extra
                )
            }
            Self::Document => {
                let d = document_path(x, y, width, height);
                format!(
                    r##"<path d="{d}" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width}" {extra}/>"##,
                    d = d, fill = style.fill, stroke = style.stroke, stroke_width = style.stroke_width, extra = extra
                )
            }
            Self::Cloud => {
                let points = polygon_points(&cloud_points(x, y, width, height));
                format!(
                    r##"<polygon points="{points}" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width}" {extra}/>"##,
                    points = points, fill = style.fill, stroke = style.stroke, stroke_width = style.stroke_width, extra = extra
                )
            }
            Self::Subprocess => {
                let (ix, iy, iw, ih) = subprocess_inset(x, y, width, height);
                format!(
                    r##"<rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{fill}" stroke="{stroke}" stroke-width="{stroke_width}" {extra}/>
<rect x="{ix}" y="{iy}" width="{iw}" height="{ih}" fill="none" stroke="{stroke}" stroke-width="{stroke_width}" {extra}/>"##,
                    x = x, y = y, w = width, h = height,
                    ix = ix, iy = iy, iw = iw, ih = ih,
                    fill = style.fill, stroke = style.stroke, stroke_width = style.stroke_width, extra = extra
                )
            }
        }
    }
}

fn polygon_points(points: &[crate::graphic_style::common::Point]) -> String {
    points
        .iter()
        .map(|p| format!("{},{}", p.x, p.y))
        .collect::<Vec<_>>()
        .join(" ")
}

fn node_svg_attrs(style: &NodeStyle, cx: f64, cy: f64) -> String {
    let mut attrs = Vec::new();
    if let Some(dash) = &style.stroke_dasharray {
        attrs.push(format!(r#"stroke-dasharray="{dash}""#));
    }
    if let Some(linecap) = &style.stroke_linecap {
        attrs.push(format!(r#"stroke-linecap="{linecap}""#));
    }
    if let Some(linejoin) = &style.stroke_linejoin {
        attrs.push(format!(r#"stroke-linejoin="{linejoin}""#));
    }
    if let Some(transform) = &style.transform {
        attrs.push(crate::graphic_style::common::centered_node_transform(transform, cx, cy));
    }
    attrs.join(" ")
}
