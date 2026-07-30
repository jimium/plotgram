//! Render representative content-block samples to SVG + ContentLayout JSON
//! for offline verification (`scripts/calibrate_content_measure.py
//! verify-layout / render-layout`) and the one-page gallery
//! (`scripts/content_gallery.py`).
//!
//! Usage: cargo run -p plotgram-content --example render_samples
//! Output: target/content-samples/<name>.svg / <name>.layout.json / <name>.meta.json

use plotgram_content::{emit_svg, measure, parse, Align, ContentPaint, MeasureParams};

const PAD: f64 = 16.0;

struct Sample {
    name: &'static str,
    text: &'static str,
    /// None = natural width, Some = wrap budget.
    max_width: Option<f64>,
    align: Align,
    max_lines: Option<usize>,
}

const fn sample(name: &'static str, text: &'static str) -> Sample {
    Sample { name, text, max_width: None, align: Align::Left, max_lines: None }
}

const SAMPLES: &[Sample] = &[
    // 1. Architecture node: title + rule + unordered list + code.
    sample(
        "order_service",
        "**订单服务**\n---\n处理下单主链路：\n- 校验库存\n- 落库并发 `order.created`\n- 幂等键去重",
    ),
    // 2. Mixed CJK/Latin, emph + code in list items.
    sample(
        "api_gateway",
        "**API Gateway**\n---\n责任：鉴权、限流、路由\n- JWT 校验（RS256）\n- 限流 *1000 QPS*\n- 灰度路由 `x-canary`",
    ),
    // 3. Ordered list, authored numbers, code span.
    sample(
        "release_steps",
        "**发布流程**\n1. 构建镜像 `docker build`\n2. 推送 registry\n3. 滚动更新 *无损*",
    ),
    // 4. Paragraphs: hard breaks vs blank-line separation.
    sample(
        "timeout_notes",
        "支付超时 30s\n重试 3 次后入死信队列 DLQ\n\n*注意*：对账任务每日 02:00 执行",
    ),
    // 5. Degradation showcase: everything stays literal, nothing errors.
    sample(
        "degradation",
        "# 不是标题\n  - 缩进不算列表\n**没闭合\n3 * 4 * 5\n\\*字面星号\\* 与 \\`反引号\\`",
    ),
    // 6. Wrapping: long mixed paragraphs + list items under a 220px budget.
    Sample {
        max_width: Some(220.0),
        ..sample(
            "wrap_demo",
            "**支付回调处理**\n---\n收到网关回调后先验签，再幂等落库，失败进入重试队列并告警通知值班\n- 验签失败直接拒绝并记录 audit log 供安全团队追溯\n- `payment.callback.retry` 队列最大重试 5 次，间隔指数退避",
        )
    },
    // 7. Center alignment: state-node style label, lines of differing width.
    Sample {
        align: Align::Center,
        ..sample(
            "align_center",
            "**灰度发布**\n*canary 5% → 25% → 100%*\n指标平稳后自动放量",
        )
    },
    // 8. Truncation: wrap under 200px, keep 3 visual lines + trailing ….
    Sample {
        max_width: Some(200.0),
        max_lines: Some(3),
        ..sample(
            "truncate_demo",
            "**变更公告**\n---\n本次升级包含账务核心迁移，涉及订单、支付、结算三个域的读写切换，回滚窗口为发布后三十分钟内",
        )
    },
    // 9. Color control: per-style fills + code chips, all paint-phase.
    sample(
        "color_chips",
        "**配置中心**\n---\n灰度开关下发链路\n- `feature.flags` 生产改动需 *双人复核*\n- 回滚 `config.rollback` 秒级生效",
    ),
];

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
        strong_fill: None,                     // strong 继承正文色，只加粗
        emph_fill: Some("#4c6ef5".into()),     // emph 斜体 + 蓝
        code_fill: Some("#7c3aed".into()),     // code 等宽 + 紫
        code_chip_fill: Some("#f3effe".into()), // code 底色 chip（落在既有 run 盒上）
        rule_stroke: "#c3c9d4".into(),
        mono_family: "Menlo".into(),
    };

    let out_dir = std::path::Path::new("target/content-samples");
    std::fs::create_dir_all(out_dir).expect("create out dir");

    for s in SAMPLES {
        let name = s.name;
        let params = MeasureParams {
            max_width: s.max_width,
            align: s.align,
            max_lines: s.max_lines,
            ..base.clone()
        };
        let layout = measure(&parse(s.text), &params);
        let fragment = emit_svg(&layout, &paint);
        let (w, h) = (layout.width + PAD * 2.0, layout.height + PAD * 2.0);
        let svg = format!(
            concat!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.0}" height="{h:.0}" "#,
                r#"viewBox="0 0 {w:.0} {h:.0}">"#,
                r##"<rect x="0.5" y="0.5" width="{rw:.0}" height="{rh:.0}" rx="8" "##,
                r##"fill="#ffffff" stroke="#8a93a6"/>"##,
                r#"<g transform="translate({p},{p})">{frag}</g></svg>"#
            ),
            w = w,
            h = h,
            rw = w - 1.0,
            rh = h - 1.0,
            p = PAD,
            frag = fragment,
        );
        std::fs::write(out_dir.join(format!("{name}.svg")), svg).expect("write svg");
        std::fs::write(
            out_dir.join(format!("{name}.layout.json")),
            serde_json::to_string_pretty(&layout).expect("serialize layout"),
        )
        .expect("write layout json");
        // Sidecar for the gallery: source text + the inputs not echoed in the layout.
        let meta = serde_json::json!({
            "name": name,
            "text": s.text,
            "max_width": s.max_width,
            "align": match s.align {
                Align::Left => "left",
                Align::Center => "center",
                Align::Right => "right",
            },
            "max_lines": s.max_lines,
            "pad": PAD,
        });
        std::fs::write(
            out_dir.join(format!("{name}.meta.json")),
            serde_json::to_string_pretty(&meta).expect("serialize meta"),
        )
        .expect("write meta json");
        println!("{name}: {:.1} x {:.1} ({} lines, {} rules)", layout.width, layout.height, layout.lines.len(), layout.rules.len());
    }
    println!("wrote {} samples to {}", SAMPLES.len(), out_dir.display());
}
