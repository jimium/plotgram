//! ContentLayout + paint -> SVG `<g>` fragment.
//!
//! Pure expansion (ADR-005 §2): every coordinate comes from `ContentLayout`;
//! this module adds paint and XML plumbing only. The fragment uses local
//! coordinates — the renderer translates it to the node's final position.

use crate::ast::RunStyle;
use crate::measure::ContentLayout;

/// Paint-only inputs; render/theme-owned, must not influence geometry.
///
/// Color control is per run-style, not per run: authors mark *semantics*
/// (strong/emph/code) in the MD subset, the theme decides what each style
/// looks like. `None` = inherit `text_fill`.
#[derive(Debug, Clone, PartialEq)]
pub struct ContentPaint {
    pub text_fill: String,
    /// Fill override for `**strong**` runs.
    pub strong_fill: Option<String>,
    /// Fill override for `*emph*` runs.
    pub emph_fill: Option<String>,
    /// Fill override for `` `code` `` runs.
    pub code_fill: Option<String>,
    /// Background chip behind `code` runs. Drawn on the exact run box from
    /// the layout (x / top / width / line height) — zero new geometry.
    pub code_chip_fill: Option<String>,
    pub rule_stroke: String,
    /// Font family for `code` runs (geometry assumed it monospace).
    pub mono_family: String,
}

pub fn emit_svg(layout: &ContentLayout, paint: &ContentPaint) -> String {
    let mut out = String::from("<g>");

    for line in &layout.lines {
        if line.runs.is_empty() {
            continue;
        }
        // Chips first so text ink paints on top of them.
        if let Some(chip) = &paint.code_chip_fill {
            for run in &line.runs {
                if run.style == RunStyle::Code {
                    out.push_str(&format!(
                        r#"<rect x="{}" y="{}" width="{}" height="{}" rx="2" fill="{}"/>"#,
                        fmt(run.x),
                        fmt(line.top),
                        fmt(run.width),
                        fmt(line.height),
                        xml_escape(chip),
                    ));
                }
            }
        }
        out.push_str(&format!(
            r#"<text y="{}" font-size="{}" font-family="{}" fill="{}">"#,
            fmt(line.baseline),
            fmt(layout.font_size),
            xml_escape(&layout.font_family),
            xml_escape(&paint.text_fill),
        ));
        for run in &line.runs {
            let mut style_attrs = match run.style {
                RunStyle::Plain => String::new(),
                RunStyle::Strong => r#" font-weight="bold""#.to_string(),
                RunStyle::Emph => r#" font-style="italic""#.to_string(),
                RunStyle::Code => format!(r#" font-family="{}""#, xml_escape(&paint.mono_family)),
            };
            let fill = match run.style {
                RunStyle::Plain => None,
                RunStyle::Strong => paint.strong_fill.as_deref(),
                RunStyle::Emph => paint.emph_fill.as_deref(),
                RunStyle::Code => paint.code_fill.as_deref(),
            };
            if let Some(f) = fill {
                style_attrs.push_str(&format!(r#" fill="{}""#, xml_escape(f)));
            }
            out.push_str(&format!(
                r#"<tspan x="{}"{}>{}</tspan>"#,
                fmt(run.x),
                style_attrs,
                xml_escape(&run.text),
            ));
        }
        out.push_str("</text>");
    }

    for &y in &layout.rules {
        out.push_str(&format!(
            r#"<line x1="0" y1="{y}" x2="{w}" y2="{y}" stroke="{s}" stroke-width="{t}"/>"#,
            y = fmt(y),
            w = fmt(layout.width),
            s = xml_escape(&paint.rule_stroke),
            t = fmt(layout.rule_thickness),
        ));
    }

    out.push_str("</g>");
    out
}

/// Trim trailing zeros for stable, compact coordinates.
fn fmt(v: f64) -> String {
    let s = format!("{v:.2}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::measure::{measure, Align, MeasureParams};
    use crate::parse::parse;

    #[test]
    fn emit_expands_layout_without_new_geometry() {
        let params = MeasureParams {
            font_size: 14.0,
            line_height: 1.4,
            font_family: "Noto Sans CJK SC".into(),
            paragraph_gap: 6.0,
            list_indent: 18.0,
            rule_thickness: 1.0,
            rule_gap: 8.0,
            max_width: None,
            align: Align::Left,
            max_lines: None,
        };
        let paint = ContentPaint {
            text_fill: "#1f2430".into(),
            strong_fill: None,
            emph_fill: Some("#4c6ef5".into()),
            code_fill: Some("#7c3aed".into()),
            code_chip_fill: Some("#f3effe".into()),
            rule_stroke: "#d0d4dc".into(),
            mono_family: "Menlo".into(),
        };
        let cl = measure(&parse("**A** & `<b>` *i*\n---"), &params);
        let svg = emit_svg(&cl, &paint);

        // Observable output: styled tspans, escaped text, rule spanning width.
        assert!(svg.starts_with("<g>") && svg.ends_with("</g>"));
        assert!(svg.contains(r#"font-weight="bold""#));
        assert!(svg.contains(r##"font-family="Menlo" fill="#7c3aed""##));
        assert!(svg.contains(r##"font-style="italic" fill="#4c6ef5""##));
        // No override -> strong inherits the <text> fill, no tspan fill attr.
        assert!(svg.contains(r#"font-weight="bold">A"#));
        assert!(svg.contains("&amp;") && svg.contains("&lt;b&gt;"));
        assert!(svg.contains(r#"x2=""#));
        // Chip rect sits exactly on the code run box, painted before text.
        let code_run = cl.lines[0]
            .runs
            .iter()
            .find(|r| r.style == RunStyle::Code)
            .unwrap();
        let chip = format!(
            r##"<rect x="{}" y="{}" width="{}" height="{}" rx="2" fill="#f3effe"/>"##,
            fmt(code_run.x),
            fmt(cl.lines[0].top),
            fmt(code_run.width),
            fmt(cl.lines[0].height),
        );
        assert!(
            svg.contains(&chip),
            "chip rect must reuse the run box verbatim"
        );
        assert!(svg.find(&chip).unwrap() < svg.find("<text").unwrap());
        // No chip when the paint doesn't ask for one.
        let no_chip = ContentPaint {
            code_chip_fill: None,
            ..paint.clone()
        };
        assert!(!emit_svg(&cl, &no_chip).contains("#f3effe"));
        // Same layout + same paint -> byte-identical fragment (determinism).
        assert_eq!(svg, emit_svg(&cl, &paint));

        // Paint must never move geometry: recolor everything, coordinates and
        // structure stay byte-identical once attribute values are stripped.
        let recolor = ContentPaint {
            text_fill: "#000000".into(),
            strong_fill: None,
            emph_fill: Some("#111111".into()),
            code_fill: Some("#222222".into()),
            code_chip_fill: Some("#444444".into()),
            rule_stroke: "#333333".into(),
            mono_family: "Menlo".into(),
        };
        let strip = |s: &str| {
            s.split('"')
                .enumerate()
                .filter(|(i, _)| i % 2 == 0) // keep structure, drop attr values
                .map(|(_, p)| p)
                .collect::<String>()
        };
        assert_eq!(strip(&svg), strip(&emit_svg(&cl, &recolor)));
    }
}
