//! Shared outline sampling for sketch-mode shapes (and group frames).

use plotgram_model::geometry::{Point, Rect};

use crate::resolve::ResolvedNodeStyle;

/// One closed outline subpath: sampled points + optional fill override (e.g. `"none"`).
pub type OutlineSubpath = (Vec<Point>, Option<String>);

/// Sample shape outlines into polylines for sketch jitter.
///
/// Standard mode keeps native SVG primitives; this is only used when
/// `strategy.sample_outlines()` is true.
pub fn shape_outlines(
    shape: &str,
    frame: &Rect,
    style: &ResolvedNodeStyle,
) -> Vec<OutlineSubpath> {
    let x = frame.x;
    let y = frame.y;
    let w = frame.width;
    let h = frame.height;
    let cx = x + w / 2.0;
    let cy = y + h / 2.0;

    match shape {
        "rect" => vec![(sample_rounded_rect(x, y, w, h, style.radius.unwrap_or(0.0)), None)],
        "rounded_rect" => vec![(sample_rounded_rect(x, y, w, h, style.radius.unwrap_or(8.0)), None)],
        "stadium" => vec![(sample_rounded_rect(x, y, w, h, h.min(w) / 2.0), None)],
        "circle" => {
            let r = w.min(h) / 2.0;
            vec![(sample_ellipse(cx, cy, r, r, 20), None)]
        }
        "diamond" => vec![(
            sample_polygon(&[(cx, y), (x + w, cy), (cx, y + h), (x, cy)]),
            None,
        )],
        "hexagon" => {
            let hm = h / 2.0;
            let w4 = w / 4.0;
            vec![(
                sample_polygon(&[
                    (x + w4, y),
                    (x + w - w4, y),
                    (x + w, y + hm),
                    (x + w - w4, y + h),
                    (x + w4, y + h),
                    (x, y + hm),
                ]),
                None,
            )]
        }
        "parallelogram" => {
            let skew = w * 0.15;
            vec![(
                sample_polygon(&[(x + skew, y), (x + w, y), (x + w - skew, y + h), (x, y + h)]),
                None,
            )]
        }
        "person" => {
            let head_r = (w * 0.30).min(h * 0.24);
            let ty = y + head_r * 2.2;
            let tw = w.min(head_r * 4.4);
            let sr = (tw * 0.35).min((y + h - ty) * 0.9);
            let (lx, rx) = (cx - tw / 2.0, cx + tw / 2.0);
            let by = y + h;
            let mut torso = Vec::new();
            push_line(&mut torso, (lx, by), (lx, ty + sr));
            push_arc(&mut torso, (lx + sr, ty + sr), sr, sr, 180.0, 270.0);
            push_line(&mut torso, (lx + sr, ty), (rx - sr, ty));
            push_arc(&mut torso, (rx - sr, ty + sr), sr, sr, 270.0, 360.0);
            push_line(&mut torso, (rx, ty + sr), (rx, by));
            push_line(&mut torso, (rx, by), (lx, by));
            vec![
                (sample_ellipse(cx, y + head_r, head_r, head_r, 14), None),
                (torso, None),
            ]
        }
        "cylinder" => {
            let ry = 8.0_f64.min(h / 4.0);
            let rx = w / 2.0;
            let mut body = Vec::new();
            push_line(&mut body, (x, y + ry), (x, y + h - ry));
            push_arc(&mut body, (cx, y + h - ry), rx, ry, 180.0, 0.0);
            push_line(&mut body, (x + w, y + h - ry), (x + w, y + ry));
            push_arc(&mut body, (cx, y + ry), rx, ry, 0.0, -180.0);
            vec![
                (body, None),
                (sample_ellipse(cx, y + ry, rx, ry, 16), None),
            ]
        }
        "document" => {
            let wave_y = y + h * 0.85;
            let wave_h = h * 0.15;
            let mut pts = Vec::new();
            push_line(&mut pts, (x, y), (x + w, y));
            push_line(&mut pts, (x + w, y), (x + w, wave_y));
            let n = 12;
            for i in 0..n {
                let u = i as f64 / n as f64;
                let px = x + w - u * w;
                let py = wave_y - 0.6 * wave_h * (std::f64::consts::TAU * u).sin();
                pts.push(Point { x: px, y: py });
            }
            push_line(&mut pts, (x, wave_y), (x, y));
            vec![(pts, None)]
        }
        "cloud" => {
            let ccy = y + h * 0.55;
            let rx = w / 2.0;
            let ry = h * 0.42;
            let samples = 28;
            let tau = std::f64::consts::TAU;
            let pts = (0..samples)
                .map(|i| {
                    let t = i as f64 / samples as f64 * tau;
                    let r_mod = 1.0 + 0.2 * (3.0 * t).sin() + 0.14 * (5.0 * t + 0.8).cos();
                    Point {
                        x: cx + rx * t.cos() * r_mod,
                        y: ccy + ry * t.sin() * r_mod,
                    }
                })
                .collect();
            vec![(pts, None)]
        }
        "subprocess" => {
            let pad = (w.min(h) * 0.08).clamp(4.0, 10.0);
            vec![
                (sample_polygon(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)]), None),
                (
                    sample_polygon(&[
                        (x + pad, y + pad),
                        (x + w - pad, y + pad),
                        (x + w - pad, y + h - pad),
                        (x + pad, y + h - pad),
                    ]),
                    Some("none".to_string()),
                ),
            ]
        }
        _ => vec![(sample_rounded_rect(x, y, w, h, style.radius.unwrap_or(4.0)), None)],
    }
}

/// Max segment length when subdividing straight edges (px).
const SAMPLE_STEP: f64 = 22.0;

fn sample_polygon(vertices: &[(f64, f64)]) -> Vec<Point> {
    let mut pts = Vec::new();
    for i in 0..vertices.len() {
        let a = vertices[i];
        let b = vertices[(i + 1) % vertices.len()];
        push_line(&mut pts, a, b);
    }
    pts
}

fn push_line(pts: &mut Vec<Point>, a: (f64, f64), b: (f64, f64)) {
    let len = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
    let segs = (len / SAMPLE_STEP).ceil().max(1.0) as usize;
    for i in 0..segs {
        let t = i as f64 / segs as f64;
        pts.push(Point {
            x: a.0 + (b.0 - a.0) * t,
            y: a.1 + (b.1 - a.1) * t,
        });
    }
}

fn push_arc(
    pts: &mut Vec<Point>,
    center: (f64, f64),
    rx: f64,
    ry: f64,
    from_deg: f64,
    to_deg: f64,
) {
    let segs = 4;
    for i in 0..segs {
        let t = i as f64 / segs as f64;
        let ang = (from_deg + (to_deg - from_deg) * t).to_radians();
        pts.push(Point {
            x: center.0 + rx * ang.cos(),
            y: center.1 + ry * ang.sin(),
        });
    }
}

fn sample_ellipse(cx: f64, cy: f64, rx: f64, ry: f64, n: usize) -> Vec<Point> {
    (0..n)
        .map(|i| {
            let t = i as f64 / n as f64 * std::f64::consts::TAU;
            Point {
                x: cx + rx * t.cos(),
                y: cy + ry * t.sin(),
            }
        })
        .collect()
}

/// Rounded rectangle outline (arcs sampled); r == 0 degrades to a rect.
pub fn sample_rounded_rect(x: f64, y: f64, w: f64, h: f64, r: f64) -> Vec<Point> {
    let r = r.min(w / 2.0).min(h / 2.0);
    if r < 0.5 {
        return sample_polygon(&[(x, y), (x + w, y), (x + w, y + h), (x, y + h)]);
    }
    let mut pts = Vec::new();
    push_line(&mut pts, (x + r, y), (x + w - r, y));
    push_arc(&mut pts, (x + w - r, y + r), r, r, -90.0, 0.0);
    push_line(&mut pts, (x + w, y + r), (x + w, y + h - r));
    push_arc(&mut pts, (x + w - r, y + h - r), r, r, 0.0, 90.0);
    push_line(&mut pts, (x + w - r, y + h), (x + r, y + h));
    push_arc(&mut pts, (x + r, y + h - r), r, r, 90.0, 180.0);
    push_line(&mut pts, (x, y + h - r), (x, y + r));
    push_arc(&mut pts, (x + r, y + r), r, r, 180.0, 270.0);
    pts
}

/// Closed SVG path `d` from points (M ... L ... Z).
pub fn closed_path_d(points: &[Point]) -> String {
    let mut d = String::new();
    for (i, p) in points.iter().enumerate() {
        if i == 0 {
            d.push_str(&format!("M {:.1} {:.1}", p.x, p.y));
        } else {
            d.push_str(&format!(" L {:.1} {:.1}", p.x, p.y));
        }
    }
    d.push_str(" Z");
    d
}

#[cfg(test)]
mod tests {
    use super::*;
    use plotgram_model::attr::AttrMap;
    use plotgram_model::graph::Node;

    #[test]
    fn sampled_outlines_are_dense_finite_and_bounded() {
        let theme = crate::theme::load(None);
        let style = crate::resolve::resolve_node(
            &Node {
                id: "n".to_string(),
                label: None,
                shape: None,
                role: Default::default(),
                host_group: None,
                anchor: None,
                attrs: AttrMap::new(),
            },
            &theme,
        );
        let frame = Rect::new(100.0, 50.0, 120.0, 60.0);

        // (shape, subpath count, allowed overshoot beyond frame in px).
        // Cloud bulges past its frame by design (r_mod peaks at ~1.34);
        // every other shape must stay inside. This is the safety net against
        // sketch-vs-standard geometry drift (the hexagon lesson).
        let cases = [
            ("rect", 1, 0.5),
            ("rounded_rect", 1, 0.5),
            ("stadium", 1, 0.5),
            ("circle", 1, 0.5),
            ("diamond", 1, 0.5),
            ("hexagon", 1, 0.5),
            ("parallelogram", 1, 0.5),
            ("person", 2, 0.5),
            ("cylinder", 2, 0.5),
            ("document", 1, 0.5),
            ("cloud", 1, 45.0),
            ("subprocess", 2, 0.5),
        ];
        for (shape, subpaths, overshoot) in cases {
            let outlines = shape_outlines(shape, &frame, &style);
            assert_eq!(outlines.len(), subpaths, "{shape}: subpath count");
            for (pts, _) in &outlines {
                assert!(pts.len() >= 8, "{shape}: sampling too sparse ({} pts)", pts.len());
                for p in pts {
                    assert!(p.x.is_finite() && p.y.is_finite(), "{shape}: NaN/inf point");
                    assert!(
                        p.x >= frame.x - overshoot
                            && p.x <= frame.x + frame.width + overshoot
                            && p.y >= frame.y - overshoot
                            && p.y <= frame.y + frame.height + overshoot,
                        "{shape}: point ({:.1}, {:.1}) escapes frame",
                        p.x,
                        p.y
                    );
                }
            }
        }

        // Subprocess inner frame is a stroke-only accent: repainting fill
        // there would cover the hatch pattern in sketch mode.
        let sp = shape_outlines("subprocess", &frame, &style);
        assert_eq!(sp[1].1.as_deref(), Some("none"));
    }
}
