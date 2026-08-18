//! Native micro-benchmark for the content pipeline (parse / measure / emit).
//!
//! Usage: cargo run -p tautcore-content --release --example content_bench
//! Examples are never compiled to WASM, so raw `Instant` is fine here.

use std::hint::black_box;
use std::time::Instant;

use tautcore_content::{emit_svg, measure, parse, Align, ContentPaint, MeasureParams};

/// Median-of-batches wall time per op, in nanoseconds.
fn bench(iters: u32, mut f: impl FnMut()) -> f64 {
    const BATCHES: usize = 7;
    let mut samples: Vec<f64> = (0..BATCHES)
        .map(|_| {
            let t0 = Instant::now();
            for _ in 0..iters {
                f();
            }
            t0.elapsed().as_nanos() as f64 / iters as f64
        })
        .collect();
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    samples[BATCHES / 2]
}

fn fmt_ns(ns: f64) -> String {
    if ns < 1_000.0 {
        format!("{ns:.0} ns")
    } else if ns < 1_000_000.0 {
        format!("{:.2} µs", ns / 1_000.0)
    } else {
        format!("{:.2} ms", ns / 1_000_000.0)
    }
}

fn main() {
    let base = MeasureParams {
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
        rule_stroke: "#c3c9d4".into(),
        mono_family: "Menlo".into(),
    };

    // Representative node (the api_gateway gallery sample).
    let node = "**API Gateway**\n---\n责任：鉴权、限流、路由\n- JWT 校验（RS256）\n- 限流 *1000 QPS*\n- 灰度路由 `x-canary`";
    // Repeated mixed block: lint threshold scale (~64 lines) and stress scale.
    let unit = "**支付回调处理**\n---\n收到网关回调后先验签，再幂等落库，失败进入重试队列并告警通知值班\n- 验签失败直接拒绝并记录 audit log 供安全团队追溯\n- `payment.callback.retry` 队列最大重试 5 次，间隔指数退避\n\n";
    let doc64 = unit.repeat(11); // 66 source lines
    let doc1k = unit.repeat(171); // 1026 source lines

    let wrap = |mw: f64| MeasureParams {
        max_width: Some(mw),
        ..base.clone()
    };
    let trunc = MeasureParams {
        max_width: Some(220.0),
        max_lines: Some(3),
        ..base.clone()
    };

    println!("tautcore-content release bench (median of 7 batches)");
    println!("{}", "-".repeat(72));
    println!(
        "{:<34} {:>10} {:>12} {:>12}",
        "case", "iters", "per op", "throughput"
    );

    let cases: Vec<(&str, u32, Box<dyn FnMut()>)> = vec![
        ("parse: node (6 lines)", 100_000, {
            Box::new(move || {
                black_box(parse(black_box(node)));
            })
        }),
        ("measure: node, no wrap", 100_000, {
            let (doc, p) = (parse(node), base.clone());
            Box::new(move || {
                black_box(measure(black_box(&doc), &p));
            })
        }),
        ("measure: node, wrap 220px", 100_000, {
            let (doc, p) = (parse(node), wrap(220.0));
            Box::new(move || {
                black_box(measure(black_box(&doc), &p));
            })
        }),
        ("measure: node, wrap+trunc+center", 100_000, {
            let (doc, p) = (
                parse(node),
                MeasureParams {
                    align: Align::Center,
                    ..trunc.clone()
                },
            );
            Box::new(move || {
                black_box(measure(black_box(&doc), &p));
            })
        }),
        ("emit: node", 100_000, {
            let (lay, pt) = (measure(&parse(node), &base), paint.clone());
            Box::new(move || {
                black_box(emit_svg(black_box(&lay), &pt));
            })
        }),
        ("full: node parse+measure+emit", 50_000, {
            let (p, pt) = (base.clone(), paint.clone());
            Box::new(move || {
                let lay = measure(&parse(black_box(node)), &p);
                black_box(emit_svg(&lay, &pt));
            })
        }),
        ("full: 66-line doc, wrap 220px", 5_000, {
            let (text, p, pt) = (doc64.clone(), wrap(220.0), paint.clone());
            Box::new(move || {
                let lay = measure(&parse(black_box(&text)), &p);
                black_box(emit_svg(&lay, &pt));
            })
        }),
        ("full: 1026-line doc, wrap 220px", 300, {
            let (text, p, pt) = (doc1k.clone(), wrap(220.0), paint.clone());
            Box::new(move || {
                let lay = measure(&parse(black_box(&text)), &p);
                black_box(emit_svg(&lay, &pt));
            })
        }),
    ];

    for (name, iters, mut f) in cases {
        let ns = bench(iters, &mut f);
        let ops = 1e9 / ns;
        println!(
            "{:<34} {:>10} {:>12} {:>9.0} op/s",
            name,
            iters,
            fmt_ns(ns),
            ops
        );
    }
}
