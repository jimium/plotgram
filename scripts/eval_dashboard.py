#!/usr/bin/env python3
"""
drawify-eval 算法评估看板

纯 HTML/CSS 实现，零外部依赖，双击即可打开。
布局算法和路由算法分开展示，避免笛卡尔积干扰。

用法:
  python3 scripts/eval_dashboard.py bench_result.json -o target/eval-dashboard/index.html
"""

import json
import argparse
from collections import defaultdict


# ── 映射表 ──

TYPE_LABELS = {
    "flowchart": "流程图", "architecture": "架构图", "state": "状态图",
    "er": "ER图", "sequence": "时序图", "mindmap": "思维导图",
}
GRADE_LABELS = {"Excellent": "优秀", "Good": "良好", "Acceptable": "可接受", "Poor": "较差"}
DIM_LABELS = {"correctness": "正确性", "compactness": "紧凑性", "uniformity": "均匀性", "aesthetics": "美观性"}
DIM_ORDER = ["correctness", "compactness", "uniformity", "aesthetics"]
DIM_COLORS = {"correctness": "#3b82f6", "compactness": "#8b5cf6", "uniformity": "#06b6d4", "aesthetics": "#f59e0b"}

TYPE_ICONS = {
    "flowchart": "⬡", "architecture": "⬢", "state": "◈",
    "er": "◇", "sequence": "▬", "mindmap": "✦",
}


def load_data(path):
    with open(path, "r", encoding="utf-8") as f:
        return json.load(f)


def extract_results(data):
    """提取所有评估结果，区分布局和路由"""
    layout_results = []
    routing_results = []
    seen = set()
    for comp in data.get("comparisons", []):
        dname = comp["diagram_name"]
        dtype = comp["diagram_type"]
        is_routing = dname.endswith(" [routing]")
        clean_name = dname.replace(" [routing]", "") if is_routing else dname
        for r in comp["results"]:
            key = (clean_name, r["algorithm"])
            if key not in seen:
                r["_diagram_name"] = clean_name
                r["_diagram_type"] = dtype
                r["_is_routing"] = is_routing
                if is_routing:
                    routing_results.append(r)
                else:
                    layout_results.append(r)
                seen.add(key)
    return layout_results, routing_results


def dim_scores(r):
    m = r["metrics"]
    ec = max(m["edge_count"], 1)
    nc = max(m["node_count"], 1)
    op = min(m["node_overlap_pairs"] / max(nc * (nc - 1) / 2, 1), 1.0)
    enp = min(m["edge_node_crossings"] / ec, 1.0)
    exp = min(m["edge_crossings"] / ec, 1.0)
    corr = (1 - op) * 0.4 + (1 - enp) * 0.35 + (1 - exp) * 0.25
    comp = min(m["area_utilization"] / 0.5, 1.0)
    unif = max(1 - m["edge_length_cv"], 0.0)
    dev = max((m["aspect_ratio"] - 1.6) / 4.0, 0.0) if m["aspect_ratio"] > 1.6 else 0.0
    aest = max(1 - dev, 0.0)
    return {k: round(v * 100, 1) for k, v in
            {"correctness": corr, "compactness": comp, "uniformity": unif, "aesthetics": aest}.items()}


def score_color(score):
    if score >= 85: return "#22c55e"
    if score >= 70: return "#3b82f6"
    if score >= 50: return "#f59e0b"
    return "#ef4444"


def avg(lst):
    return sum(lst) / len(lst) if lst else 0


def grade_of(score):
    if score >= 85: return "Excellent"
    if score >= 70: return "Good"
    if score >= 50: return "Acceptable"
    return "Poor"


def bar_html(value, max_val=100, color=None, height=8):
    pct = min(value / max_val * 100, 100) if max_val > 0 else 0
    c = color or score_color(value)
    return f'<div style="height:{height}px;border-radius:{height//2}px;background:#e5e7eb;min-width:60px"><div style="height:100%;width:{pct:.1f}%;border-radius:{height//2}px;background:{c};transition:width .3s"></div></div>'


def donut_html(segments, size=120, stroke=16):
    total = sum(v for _, v, _ in segments)
    if total == 0:
        return '<div style="text-align:center;color:#9ca3af">无数据</div>'
    offset = 0
    circles = ""
    for label, value, color in segments:
        pct = value / total * 100
        dash = pct * 3.14159 * (size - stroke) / 50
        gap = 3.14159 * (size - stroke) / 50 - dash
        circles += f'<circle cx="{size/2}" cy="{size/2}" r="{(size-stroke)/2}" fill="none" stroke="{color}" stroke-width="{stroke}" stroke-dasharray="{dash:.2f} {gap:.2f}" stroke-dashoffset="-{offset:.2f}" transform="rotate(-90 {size/2} {size/2})"/>'
        offset += dash + gap
    legend = "".join(
        f'<div style="display:flex;align-items:center;gap:6px;font-size:12px"><span style="width:8px;height:8px;border-radius:2px;background:{c};flex-shrink:0"></span><span style="color:#6b7280">{l}</span><span style="font-weight:600">{v}</span></div>'
        for l, v, c in segments
    )
    return f"""<div style="display:flex;align-items:center;gap:20px;flex-wrap:wrap">
<svg width="{size}" height="{size}" viewBox="0 0 {size} {size}">{circles}</svg>
<div style="display:flex;flex-direction:column;gap:4px">{legend}</div>
</div>"""


def hbar_chart_html(items, max_val=100):
    rows = ""
    for label, value in items:
        c = score_color(value)
        pct = min(value / max_val * 100, 100) if max_val > 0 else 0
        rows += f"""<div style="display:flex;align-items:center;gap:8px;margin-bottom:6px">
  <span style="min-width:120px;font-size:12px;color:#374151;text-align:right;white-space:nowrap;overflow:hidden;text-overflow:ellipsis" title="{label}">{label}</span>
  <div style="flex:1;height:18px;border-radius:4px;background:#f3f4f6;position:relative;overflow:hidden">
    <div style="height:100%;width:{pct:.1f}%;background:{c};border-radius:4px;transition:width .3s"></div>
    <span style="position:absolute;right:6px;top:50%;transform:translateY(-50%);font-size:11px;font-weight:600;color:{'#fff' if pct > 20 else '#374151'}">{value:.1f}</span>
  </div>
</div>
"""
    return f'<div style="width:100%">{rows}</div>'


def dim_bars_html(dim_avgs):
    rows = ""
    for dk in DIM_ORDER:
        v = dim_avgs.get(dk, 0)
        c = DIM_COLORS.get(dk, "#3b82f6")
        label = DIM_LABELS[dk]
        rows += f"""<div style="display:flex;align-items:center;gap:6px;margin-bottom:4px">
  <span style="min-width:48px;font-size:11px;color:#6b7280">{label}</span>
  <div style="flex:1;height:14px;border-radius:3px;background:#f3f4f6;overflow:hidden">
    <div style="height:100%;width:{v}%;background:{c};border-radius:3px"></div>
  </div>
  <span style="min-width:28px;font-size:11px;font-weight:600;color:{score_color(v)};text-align:right">{v:.1f}</span>
</div>
"""
    return f'<div style="width:100%">{rows}</div>'


def extract_routing_name(algo_name):
    """从 'sugiyama+orthogonal' 提取路由名 'orthogonal'"""
    if "+" in algo_name:
        return algo_name.split("+", 1)[1]
    return None


def build_algo_section(results, section_label, is_routing=False):
    """构建一个算法板块（布局或路由）的完整数据"""
    if not results:
        return None

    # 按图类型分组
    type_algo = defaultdict(lambda: defaultdict(list))
    for r in results:
        # 路由对比时，算法名是 layout+routing，展示时只取路由名
        display_name = extract_routing_name(r["algorithm"]) if is_routing else r["algorithm"]
        if display_name is None:
            display_name = r["algorithm"]
        r["_display_name"] = display_name
        type_algo[r["_diagram_type"]][display_name].append(r["score"])

    # 按图类型推荐
    type_best = {}
    for dtype, algo_map in type_algo.items():
        ranked = sorted(algo_map.items(), key=lambda x: avg(x[1]), reverse=True)
        best_algo, best_scores = ranked[0]
        type_best[dtype] = {
            "algo": best_algo,
            "score": round(avg(best_scores), 1),
            "grade": grade_of(avg(best_scores)),
            "count": len(best_scores),
            "all_algos": len(algo_map),
        }

    # 算法画像
    algo_profile = defaultdict(lambda: {"scores": [], "elapsed": [], "dims": defaultdict(list),
                                         "types": defaultdict(list)})
    for r in results:
        a = r["_display_name"]
        p = algo_profile[a]
        p["scores"].append(r["score"])
        p["elapsed"].append(r["elapsed_us"])
        ds = dim_scores(r)
        for k, v in ds.items():
            p["dims"][k].append(v)
        p["types"][r["_diagram_type"]].append(r["score"])

    algo_names = sorted(algo_profile.keys(), key=lambda a: avg(algo_profile[a]["scores"]), reverse=True)

    algo_strength = {}
    for a in algo_names:
        p = algo_profile[a]
        type_avgs = {t: round(avg(s), 1) for t, s in p["types"].items()}
        sorted_types = sorted(type_avgs.items(), key=lambda x: x[1], reverse=True)
        best_type = sorted_types[0] if sorted_types else ("-", 0)
        worst_type = sorted_types[-1] if sorted_types else ("-", 0)
        dim_avgs = {k: round(avg(v), 1) for k, v in p["dims"].items()}
        weakest_dim = min(dim_avgs, key=dim_avgs.get) if dim_avgs else "correctness"
        algo_strength[a] = {
            "avg_score": round(avg(p["scores"]), 1),
            "avg_elapsed": round(avg(p["elapsed"]), 0),
            "best_type": TYPE_LABELS.get(best_type[0], best_type[0]),
            "best_type_score": best_type[1],
            "worst_type": TYPE_LABELS.get(worst_type[0], worst_type[0]),
            "worst_type_score": worst_type[1],
            "weakest_dim": DIM_LABELS[weakest_dim],
            "weakest_dim_score": dim_avgs.get(weakest_dim, 0),
            "grade": grade_of(avg(p["scores"])),
            "dim_avgs": dim_avgs,
            "type_avgs": type_avgs,
        }

    return {
        "type_algo": type_algo,
        "type_best": type_best,
        "algo_names": algo_names,
        "algo_strength": algo_strength,
        "algo_profile": algo_profile,
    }


def generate_html(data):
    layout_results, routing_results = extract_results(data)
    all_results = layout_results + routing_results

    if not all_results:
        return "<html><body><h1>无数据</h1></body></html>"

    layout_sec = build_algo_section(layout_results, "布局算法", is_routing=False)
    routing_sec = build_algo_section(routing_results, "路由算法", is_routing=True)

    # ── 总览数据 ──
    grade_counts = defaultdict(int)
    for r in all_results:
        grade_counts[r["quality_grade"]] += 1
    dist_segments = [
        ("优秀(≥85)", grade_counts.get("Excellent", 0), "#22c55e"),
        ("良好(70-85)", grade_counts.get("Good", 0), "#3b82f6"),
        ("可接受(50-70)", grade_counts.get("Acceptable", 0), "#f59e0b"),
        ("较差(<50)", grade_counts.get("Poor", 0), "#ef4444"),
    ]

    # ── 构建 HTML ──

    html = f"""<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Drawify 算法评估看板</title>
<style>
:root {{
  --bg: #fafafa; --card: #fff; --border: #e5e7eb;
  --text: #1f2937; --text2: #6b7280; --text3: #9ca3af;
  --green: #22c55e; --blue: #3b82f6; --amber: #f59e0b; --red: #ef4444;
  --green-bg: #f0fdf4; --blue-bg: #eff6ff; --amber-bg: #fffbeb; --red-bg: #fef2f2;
}}
* {{ margin:0; padding:0; box-sizing:border-box; }}
body {{ font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif; background: var(--bg); color: var(--text); line-height: 1.5; }}
.header {{ background: #fff; border-bottom: 1px solid var(--border); padding: 20px 32px; position: sticky; top: 0; z-index: 100; }}
.header h1 {{ font-size: 20px; font-weight: 700; }}
.header p {{ font-size: 13px; color: var(--text2); margin-top: 4px; }}
.tabs {{ display: flex; gap: 0; margin-top: 16px; }}
.tab {{ padding: 8px 20px; font-size: 14px; font-weight: 500; color: var(--text2); cursor: pointer; border-bottom: 2px solid transparent; transition: all .15s; }}
.tab:hover {{ color: var(--text); }}
.tab.active {{ color: var(--blue); border-bottom-color: var(--blue); }}
.page {{ display: none; padding: 24px 32px; max-width: 1200px; margin: 0 auto; }}
.page.active {{ display: block; }}
.section-title {{ font-size: 16px; font-weight: 600; margin-bottom: 16px; padding-bottom: 8px; border-bottom: 1px solid var(--border); }}
.section-subtitle {{ font-size: 13px; color: var(--text2); margin-bottom: 16px; }}

.type-grid {{ display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 16px; margin-bottom: 32px; }}
.type-card {{ background: var(--card); border: 1px solid var(--border); border-radius: 12px; padding: 20px; transition: box-shadow .15s; }}
.type-card:hover {{ box-shadow: 0 4px 12px rgba(0,0,0,.08); }}
.type-card .icon {{ font-size: 24px; margin-bottom: 8px; }}
.type-card .type-name {{ font-size: 16px; font-weight: 600; margin-bottom: 12px; }}
.type-card .best-algo {{ display: flex; align-items: center; gap: 8px; padding: 8px 12px; border-radius: 8px; margin-bottom: 8px; font-weight: 600; }}
.type-card .best-algo.excellent {{ background: var(--green-bg); color: #166534; }}
.type-card .best-algo.good {{ background: var(--blue-bg); color: #1e40af; }}
.type-card .best-algo.acceptable {{ background: var(--amber-bg); color: #92400e; }}
.type-card .best-algo.poor {{ background: var(--red-bg); color: #991b1b; }}
.type-card .meta {{ font-size: 12px; color: var(--text3); }}

.type-section {{ background: var(--card); border: 1px solid var(--border); border-radius: 12px; padding: 24px; margin-bottom: 20px; }}
.type-section h3 {{ font-size: 15px; font-weight: 600; margin-bottom: 16px; }}
.rank-table {{ width: 100%; border-collapse: collapse; font-size: 13px; }}
.rank-table th {{ text-align: left; padding: 8px 12px; font-size: 11px; font-weight: 600; color: var(--text3); text-transform: uppercase; letter-spacing: .5px; border-bottom: 2px solid var(--border); }}
.rank-table td {{ padding: 10px 12px; border-bottom: 1px solid var(--border); }}
.rank-table tr:last-child td {{ border-bottom: none; }}
.rank-table tr:hover {{ background: #f9fafb; }}
.rank-num {{ display: inline-flex; align-items: center; justify-content: center; width: 24px; height: 24px; border-radius: 6px; font-size: 12px; font-weight: 700; }}
.rank-1 {{ background: #fef3c7; color: #92400e; }}
.rank-2 {{ background: #f3f4f6; color: #374151; }}
.rank-3 {{ background: #ffedd5; color: #9a3412; }}
.badge {{ display: inline-block; padding: 2px 8px; border-radius: 4px; font-size: 11px; font-weight: 600; }}
.badge-Excellent {{ background: var(--green-bg); color: #166534; }}
.badge-Good {{ background: var(--blue-bg); color: #1e40af; }}
.badge-Acceptable {{ background: var(--amber-bg); color: #92400e; }}
.badge-Poor {{ background: var(--red-bg); color: #991b1b; }}

.algo-grid {{ display: grid; grid-template-columns: repeat(auto-fill, minmax(340px, 1fr)); gap: 16px; }}
.algo-card {{ background: var(--card); border: 1px solid var(--border); border-radius: 12px; padding: 20px; }}
.algo-card .algo-name {{ font-size: 15px; font-weight: 700; margin-bottom: 4px; }}
.algo-card .algo-score {{ font-size: 28px; font-weight: 800; margin-bottom: 12px; }}
.algo-card .stat-row {{ display: flex; gap: 16px; margin-bottom: 12px; font-size: 12px; color: var(--text2); }}
.algo-card .strength, .algo-card .weakness {{ font-size: 13px; padding: 8px 12px; border-radius: 8px; margin-bottom: 8px; }}
.algo-card .strength {{ background: var(--green-bg); color: #166534; }}
.algo-card .weakness {{ background: var(--amber-bg); color: #92400e; }}

.filter-bar {{ display: flex; gap: 12px; margin-bottom: 16px; flex-wrap: wrap; }}
.filter-bar select {{ padding: 6px 12px; border: 1px solid var(--border); border-radius: 6px; font-size: 13px; background: #fff; color: var(--text); }}
.detail-table-wrap {{ background: var(--card); border: 1px solid var(--border); border-radius: 12px; overflow: hidden; }}
.detail-table {{ width: 100%; border-collapse: collapse; font-size: 13px; }}
.detail-table th {{ text-align: left; padding: 10px 12px; font-size: 11px; font-weight: 600; color: var(--text3); text-transform: uppercase; letter-spacing: .5px; border-bottom: 2px solid var(--border); position: sticky; top: 0; background: #fff; z-index: 1; }}
.detail-table td {{ padding: 8px 12px; border-bottom: 1px solid var(--border); }}
.detail-table tr:hover {{ background: #f9fafb; }}
.scroll-area {{ max-height: 600px; overflow-y: auto; }}
.divider {{ border: none; border-top: 1px solid var(--border); margin: 32px 0; }}
</style>
</head>
<body>

<div class="header">
  <h1>Drawify 算法评估看板</h1>
  <p>{len(all_results)} 条评估 · {len(layout_results)} 布局 + {len(routing_results)} 路由 · {len(set(r['_diagram_type'] for r in all_results))} 种图类型</p>
  <div class="tabs">
    <div class="tab active" data-tab="overview">总览</div>
    <div class="tab" data-tab="layout">布局算法</div>
    <div class="tab" data-tab="routing">路由算法</div>
    <div class="tab" data-tab="detail">明细数据</div>
  </div>
</div>

<!-- ═══ Tab 1: 总览 ═══ -->
<div id="page-overview" class="page active">
  <div class="section-title">各图类型推荐算法</div>
  <div class="type-grid">
"""

    # ── 总览卡片：布局 + 路由推荐 ──
    if layout_sec:
        for dtype in sorted(layout_sec["type_best"].keys()):
            b = layout_sec["type_best"][dtype]
            icon = TYPE_ICONS.get(dtype, "◻")
            label = TYPE_LABELS.get(dtype, dtype)
            gc = b["grade"].lower()
            # 路由推荐
            r_best = routing_sec["type_best"].get(dtype) if routing_sec else None
            routing_html = ""
            if r_best:
                rgc = r_best["grade"].lower()
                routing_html = f"""<div class="best-algo {rgc}" style="margin-bottom:0">
          <span style="font-size:14px">{r_best['score']}</span>
          <span style="font-size:12px">路由: {r_best['algo']}</span>
        </div>"""

            html += f"""    <div class="type-card">
      <div class="icon">{icon}</div>
      <div class="type-name">{label}</div>
      <div class="best-algo {gc}">
        <span style="font-size:18px">{b['score']}</span>
        <span>布局: {b['algo']}</span>
      </div>
      {routing_html}
      <div class="meta">测试 {b['count']} 个场景 · 对比 {b['all_algos']} 种布局</div>
    </div>
"""

    # 总览: 评分分布 + 布局排名
    layout_rank_items = [(a, round(avg(layout_sec["algo_profile"][a]["scores"]), 1))
                         for a in layout_sec["algo_names"]] if layout_sec else []

    html += f"""  </div>
  <div style="display:grid;grid-template-columns:1fr 1fr;gap:20px;margin-bottom:32px">
    <div style="background:var(--card);border:1px solid var(--border);border-radius:12px;padding:20px">
      <div class="section-title" style="margin-bottom:12px">评分分布</div>
      {donut_html(dist_segments)}
    </div>
    <div style="background:var(--card);border:1px solid var(--border);border-radius:12px;padding:20px">
      <div class="section-title" style="margin-bottom:12px">布局算法综合排名</div>
      {hbar_chart_html(layout_rank_items)}
    </div>
  </div>
</div>

"""

    # ═══ Tab 2: 布局算法 ═══
    html += """<!-- ═══ Tab 2: 布局算法 ═══ -->
<div id="page-layout" class="page">
"""
    if layout_sec:
        html += """  <div class="section-subtitle">固定路由为默认，只对比布局算法</div>
"""
        for dtype in sorted(layout_sec["type_algo"].keys()):
            label = TYPE_LABELS.get(dtype, dtype)
            icon = TYPE_ICONS.get(dtype, "◻")
            algo_map = layout_sec["type_algo"][dtype]
            ranked = sorted(algo_map.items(), key=lambda x: avg(x[1]), reverse=True)
            bar_items = [(a, round(avg(s), 1)) for a, s in ranked]

            html += f"""  <div class="type-section">
    <h3>{icon} {label} — 布局算法排名</h3>
    <table class="rank-table">
      <thead><tr><th>#</th><th>算法</th><th>评分</th><th>等级</th><th>正确性</th><th>紧凑性</th><th>均匀性</th><th>美观性</th><th>样本</th></tr></thead>
    <tbody>
"""
            for i, (algo, scores) in enumerate(ranked):
                rank_cls = f"rank-{i+1}" if i < 3 else ""
                a_score = round(avg(scores), 1)
                grade = grade_of(a_score)
                p = layout_sec["algo_profile"][algo]
                dim_vals = {k: round(avg(v), 1) for k, v in p["dims"].items()}
                dim_cells = ""
                for dk in DIM_ORDER:
                    dv = dim_vals.get(dk, 0)
                    dc = score_color(dv)
                    dim_cells += f'<td><div style="display:flex;align-items:center;gap:4px">{bar_html(dv, height=5, color=dc)}<span style="font-size:11px;color:var(--text3);min-width:28px">{dv}</span></div></td>'
                html += f"""      <tr>
        <td><span class="rank-num {rank_cls}">{i+1}</span></td>
        <td style="font-weight:600">{algo}</td>
        <td style="font-weight:700;color:{score_color(a_score)}">{a_score}</td>
        <td><span class="badge badge-{grade}">{GRADE_LABELS[grade]}</span></td>
        {dim_cells}
        <td style="color:var(--text3)">{len(scores)}</td>
      </tr>
"""

            html += f"""    </tbody></table>
    <div style="margin-top:16px">{hbar_chart_html(bar_items)}</div>
  </div>
"""

        # 布局算法画像
        html += """  <hr class="divider">
  <div class="section-title">布局算法能力画像</div>
  <div class="algo-grid">
"""
        for a in layout_sec["algo_names"]:
            s = layout_sec["algo_strength"][a]
            html += f"""    <div class="algo-card">
      <div class="algo-name">{a}</div>
      <div class="algo-score" style="color:{score_color(s['avg_score'])}">{s['avg_score']}</div>
      <div class="stat-row">
        <span>⏱ {s['avg_elapsed']:.0f}μs</span>
        <span>📊 {len(layout_sec['algo_profile'][a]['scores'])} 场景</span>
      </div>
      <div class="strength">✓ 擅长 {s['best_type']}（{s['best_type_score']}分）</div>
      <div class="weakness">✗ {s['weakest_dim']}偏弱（{s['weakest_dim_score']}分），{s['worst_type']}表现差（{s['worst_type_score']}分）</div>
      <div style="margin-top:12px">
        <div style="font-size:11px;color:var(--text3);margin-bottom:6px;text-transform:uppercase;letter-spacing:.5px">维度得分</div>
        {dim_bars_html(s['dim_avgs'])}
      </div>
    </div>
"""

        html += """  </div>
</div>

"""

    # ═══ Tab 3: 路由算法 ═══
    html += """<!-- ═══ Tab 3: 路由算法 ═══ -->
<div id="page-routing" class="page">
"""
    if routing_sec:
        html += """  <div class="section-subtitle">固定布局为最佳，只对比路由算法</div>
"""
        for dtype in sorted(routing_sec["type_algo"].keys()):
            label = TYPE_LABELS.get(dtype, dtype)
            icon = TYPE_ICONS.get(dtype, "◻")
            algo_map = routing_sec["type_algo"][dtype]
            ranked = sorted(algo_map.items(), key=lambda x: avg(x[1]), reverse=True)
            bar_items = [(a, round(avg(s), 1)) for a, s in ranked]

            html += f"""  <div class="type-section">
    <h3>{icon} {label} — 路由算法排名</h3>
    <table class="rank-table">
      <thead><tr><th>#</th><th>路由</th><th>评分</th><th>等级</th><th>正确性</th><th>紧凑性</th><th>均匀性</th><th>美观性</th><th>样本</th></tr></thead>
    <tbody>
"""
            for i, (algo, scores) in enumerate(ranked):
                rank_cls = f"rank-{i+1}" if i < 3 else ""
                a_score = round(avg(scores), 1)
                grade = grade_of(a_score)
                p = routing_sec["algo_profile"][algo]
                dim_vals = {k: round(avg(v), 1) for k, v in p["dims"].items()}
                dim_cells = ""
                for dk in DIM_ORDER:
                    dv = dim_vals.get(dk, 0)
                    dc = score_color(dv)
                    dim_cells += f'<td><div style="display:flex;align-items:center;gap:4px">{bar_html(dv, height=5, color=dc)}<span style="font-size:11px;color:var(--text3);min-width:28px">{dv}</span></div></td>'
                html += f"""      <tr>
        <td><span class="rank-num {rank_cls}">{i+1}</span></td>
        <td style="font-weight:600">{algo}</td>
        <td style="font-weight:700;color:{score_color(a_score)}">{a_score}</td>
        <td><span class="badge badge-{grade}">{GRADE_LABELS[grade]}</span></td>
        {dim_cells}
        <td style="color:var(--text3)">{len(scores)}</td>
      </tr>
"""

            html += f"""    </tbody></table>
    <div style="margin-top:16px">{hbar_chart_html(bar_items)}</div>
  </div>
"""

        # 路由算法画像
        html += """  <hr class="divider">
  <div class="section-title">路由算法能力画像</div>
  <div class="algo-grid">
"""
        for a in routing_sec["algo_names"]:
            s = routing_sec["algo_strength"][a]
            html += f"""    <div class="algo-card">
      <div class="algo-name">{a}</div>
      <div class="algo-score" style="color:{score_color(s['avg_score'])}">{s['avg_score']}</div>
      <div class="stat-row">
        <span>⏱ {s['avg_elapsed']:.0f}μs</span>
        <span>📊 {len(routing_sec['algo_profile'][a]['scores'])} 场景</span>
      </div>
      <div class="strength">✓ 擅长 {s['best_type']}（{s['best_type_score']}分）</div>
      <div class="weakness">✗ {s['weakest_dim']}偏弱（{s['weakest_dim_score']}分），{s['worst_type']}表现差（{s['worst_type_score']}分）</div>
      <div style="margin-top:12px">
        <div style="font-size:11px;color:var(--text3);margin-bottom:6px;text-transform:uppercase;letter-spacing:.5px">维度得分</div>
        {dim_bars_html(s['dim_avgs'])}
      </div>
    </div>
"""

        html += """  </div>
</div>

"""

    # ═══ Tab 4: 明细 ═══
    # 合并布局+路由结果
    table_data = []
    for r in sorted(all_results, key=lambda x: x["score"], reverse=True):
        ds = dim_scores(r)
        display_name = r.get("_display_name", r["algorithm"])
        is_routing = r.get("_is_routing", False)
        table_data.append({
            "scenario": r["_diagram_name"],
            "type": r["_diagram_type"],
            "type_label": TYPE_LABELS.get(r["_diagram_type"], r["_diagram_type"]),
            "algo": display_name,
            "algo_kind": "路由" if is_routing else "布局",
            "score": r["score"],
            "grade": r["quality_grade"],
            "grade_label": GRADE_LABELS.get(r["quality_grade"], r["quality_grade"]),
            "dims": ds,
            "nodes": r["metrics"]["node_count"],
            "edges": r["metrics"]["edge_count"],
            "overlaps": r["metrics"]["node_overlap_pairs"],
            "crossings": r["metrics"]["edge_crossings"],
            "elapsed": r["elapsed_us"],
        })

    html += f"""<!-- ═══ Tab 4: 明细 ═══ -->
<div id="page-detail" class="page">
  <div class="filter-bar">
    <select id="filter-type"><option value="">全部图类型</option>
"""
    all_types = sorted(set(r["_diagram_type"] for r in all_results))
    for dtype in all_types:
        html += f"""      <option value="{dtype}">{TYPE_LABELS.get(dtype, dtype)}</option>
"""

    html += """    </select>
    <select id="filter-kind"><option value="">布局+路由</option>
      <option value="layout">仅布局</option><option value="routing">仅路由</option>
    </select>
    <select id="filter-grade"><option value="">全部等级</option>
      <option value="Excellent">优秀</option><option value="Good">良好</option>
      <option value="Acceptable">可接受</option><option value="Poor">较差</option>
    </select>
  </div>
  <div class="detail-table-wrap">
    <div class="scroll-area">
      <table class="detail-table">
        <thead><tr>
          <th>场景</th><th>图类型</th><th>类别</th><th>算法</th><th>评分</th><th>等级</th>
          <th>正确性</th><th>紧凑性</th><th>均匀性</th><th>美观性</th>
          <th>节点</th><th>边</th><th>重叠</th><th>交叉</th><th>耗时</th>
        </tr></thead>
        <tbody id="detail-body"></tbody>
      </table>
    </div>
  </div>
</div>

<script>
"""

    html += f"""var TABLE_DATA = {json.dumps(table_data, ensure_ascii=False)};
var DIM_ORDER = {json.dumps(DIM_ORDER)};

document.querySelectorAll('.tab').forEach(function(tab) {{
  tab.addEventListener('click', function() {{
    document.querySelectorAll('.tab').forEach(function(t) {{ t.classList.remove('active'); }});
    document.querySelectorAll('.page').forEach(function(p) {{ p.classList.remove('active'); }});
    tab.classList.add('active');
    document.getElementById('page-' + tab.dataset.tab).classList.add('active');
  }});
}});

function renderDetail() {{
  var ft = document.getElementById('filter-type').value;
  var fk = document.getElementById('filter-kind').value;
  var fg = document.getElementById('filter-grade').value;
  var filtered = TABLE_DATA.filter(function(r) {{
    if (ft && r.type !== ft) return false;
    if (fk === 'layout' && r.algo_kind !== '布局') return false;
    if (fk === 'routing' && r.algo_kind !== '路由') return false;
    if (fg && r.grade !== fg) return false;
    return true;
  }});
  var html = '';
  filtered.forEach(function(r) {{
    var sc = r.score >= 85 ? '#22c55e' : r.score >= 70 ? '#3b82f6' : r.score >= 50 ? '#f59e0b' : '#ef4444';
    var kindColor = r.algo_kind === '布局' ? '#3b82f6' : '#8b5cf6';
    html += '<tr>';
    html += '<td>' + r.scenario + '</td>';
    html += '<td>' + r.type_label + '</td>';
    html += '<td style="font-size:11px;font-weight:600;color:' + kindColor + '">' + r.algo_kind + '</td>';
    html += '<td style="font-weight:600">' + r.algo + '</td>';
    html += '<td style="font-weight:700;color:' + sc + '">' + r.score.toFixed(1) + '</td>';
    html += '<td><span class="badge badge-' + r.grade + '">' + r.grade_label + '</span></td>';
    DIM_ORDER.forEach(function(k) {{
      var v = r.dims[k];
      var c = v >= 85 ? '#22c55e' : v >= 70 ? '#3b82f6' : v >= 50 ? '#f59e0b' : '#ef4444';
      html += '<td style="color:' + c + '">' + v.toFixed(1) + '</td>';
    }});
    html += '<td>' + r.nodes + '</td>';
    html += '<td>' + r.edges + '</td>';
    html += '<td>' + r.overlaps + '</td>';
    html += '<td>' + r.crossings + '</td>';
    html += '<td>' + r.elapsed + '</td>';
    html += '</tr>';
  }});
  document.getElementById('detail-body').innerHTML = html;
}}
document.getElementById('filter-type').addEventListener('change', renderDetail);
document.getElementById('filter-kind').addEventListener('change', renderDetail);
document.getElementById('filter-grade').addEventListener('change', renderDetail);
renderDetail();
</script>
</body>
</html>
"""

    return html


def main():
    parser = argparse.ArgumentParser(description="Drawify 算法评估看板")
    parser.add_argument("input", help="bench 输出的 JSON 文件路径")
    parser.add_argument("-o", "--output", help="输出 HTML 文件路径")
    args = parser.parse_args()

    data = load_data(args.input)
    html = generate_html(data)

    output = args.output or "eval_dashboard.html"
    from pathlib import Path
    Path(output).parent.mkdir(parents=True, exist_ok=True)
    with open(output, "w", encoding="utf-8") as f:
        f.write(html)

    layout_results, routing_results = extract_results(data)
    print(f"看板已生成: {output}")
    print(f"  布局评估: {len(layout_results)} 条")
    print(f"  路由评估: {len(routing_results)} 条")


if __name__ == "__main__":
    main()
