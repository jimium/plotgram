//! Atlas channel 合法化探针（诊断工具，**非 CI 门禁**；27 号文 §5.1）。
//!
//! 对图集里每张图：`Diagram` → `ChannelBlueprint`（flat rank/order 网格）→
//! `Substrate`（段模型 derive）→ `ChannelGraph`，先跑 L6 组穿透检查（A1），再按
//! 三个口径采样（端点一律四侧候选端口，R3 `route_candidates` 取 LexCost 最优——
//! B7 退化轴的组内边正解是边界缝侧端口，钉死单一侧对会人为制造同 gate 双穿）：
//!
//! 1. **表达上界**：四侧候选端口（capacity=0 不限）+ gate `Unbounded` + L8
//!    作用域掩码，每边独立空占用 → 合法单边可行率（A4）；路径用独立自反证
//!    `verify_route_scope`（不复用掩码代码路径，22 号文 §8 证明义务）统计
//!    借道穿组数（A2，应恒 0）、gate 一致性违规数（A2b，应恒 0）与同 gate
//!    双穿数（A3，应恒 0）。
//! 2. **端口生产**：同 `(node, side)` 共享一个端口槽（capacity=PORT_SIDE_CAPACITY）
//!    + `Unbounded`，共享占用按声明序贪心布边（每边在四侧候选中择优）→ 端口
//!    争用成功/失败、峰值侧负载（A6）；同一次贪心给出需求画像：峰值 lane
//!    demand（A5）与峰值 gate crossing_demand（相 I 输出，不是约束）。
//! 3. **gate 诊断**：`gate_capacity_override = Fixed(cap)` 扫描（附录，不进生产
//!    结论），量化固定容量下的争用损失。
//!
//! 口径见 [`plotgram_core::layout::atlas::probe`]：统一 flat 网格，不镜像
//! divide-conquer / two_phase（有组的 flowchart/architecture 在报告中标注）。
//!
//! 用法：
//!   cargo run -p plotgram-eval --bin atlas_probe
//!   cargo run -p plotgram-eval --bin atlas_probe -- --set benchmarks/sets/product-regression-set.txt
//!   cargo run -p plotgram-eval --bin atlas_probe -- --output /tmp/atlas_probe.md

use plotgram_core::ast::Diagram;
use plotgram_core::layout::atlas::channel::{
    derive_node_ports, derive_substrate, route, route_candidates, verify_route_scope,
    ChannelGraph, DerivePortsOptions, Occupancy, PortSlotId, RouteOutcome, RouteScopeViolation,
    TrackId,
};
use plotgram_core::layout::atlas::probe::derive_channel_blueprint;
use plotgram_core::layout::kernel::cost::SolverStatus;
use plotgram_core::types::DiagramType;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// 端口侧容量（口径二）：同一 `(node, side)` 最多并发承载的边数。
/// 生产将由布局参数给出，探针取 4（节点四侧 × 4 = 单节点 16 条边上限）。
const PORT_SIDE_CAPACITY: u32 = 4;

/// 四侧候选批量挂端口（R5 [`derive_node_ports`]，同 `(node, side)` 一个槽）。
/// `capacity = 0` 表示不限量（口径一/三端口不构成约束）。
fn side_port_options(capacity: u32) -> DerivePortsOptions {
    DerivePortsOptions {
        default_capacity: capacity,
        ..DerivePortsOptions::default()
    }
}

/// 单图探针结果（A1 + 三口径 + 需求画像）。
struct DiagramProbe {
    name: String,
    dtype: DiagramType,
    nodes: usize,
    edges: usize,
    /// A1：L6 组穿透违规数（应恒 0）。
    penetrations: usize,
    // ---- 口径一：表达上界（四侧候选 cap=0 + Unbounded + L8 掩码，单边空占用） ----
    ub_converged: usize,
    ub_infeasible: Vec<(String, String)>,
    /// 端点落不到轨道（derive 网格表达不了该边）。
    unrepresentable: Vec<(String, String)>,
    /// A2：路径自反证——含无关组轨道（借道）的边数（应恒 0）。
    borrowed_passages: usize,
    /// A2b：路径自反证——gate 一致性违规（缺 gate / 配对不符 / 多余 gate /
    /// 未知轨道）的边数（应恒 0）。
    gate_inconsistencies: usize,
    /// A3：路径复核——同一 gate 出现 ≥2 次的边数（应恒 0）。
    double_crossings: usize,
    // ---- 口径二：端口生产（共享槽 cap=4 + Unbounded，共享占用贪心） ----
    port_converged: usize,
    port_failed: Vec<(String, String)>,
    /// A6：峰值端口侧负载（最拥挤 `(node, side)` 承载的边数）。
    peak_side_load: u32,
    // ---- 需求画像（口径二贪心的产物） ----
    /// A5：峰值 track lane 需求。
    peak_lane_demand: u32,
    peak_lane_track: Option<TrackId>,
    /// 峰值 gate crossing_demand（相 I 输出：最拥挤 gate 的穿越边数）。
    peak_gate_demand: u32,
    /// divide-conquer / two_phase 倾向（有组的 flowchart/architecture）。
    dc_prone: bool,
    /// flat 口径下因包围盒交叠被门面丢弃的组数（该图 A2 检验力度打折）。
    dropped_groups: usize,
    /// 蓝图排除的自环边数（L7-T5：channel 不建模，生产走节点旁小环）。
    self_loops: usize,
}

impl DiagramProbe {
    fn ub_rate(&self) -> f64 {
        if self.edges == 0 {
            1.0
        } else {
            self.ub_converged as f64 / self.edges as f64
        }
    }
}

fn try_parse_diagram(source: &str) -> Option<Diagram> {
    let raw = plotgram_core::pipeline::parse(source).ok()?;
    let output =
        plotgram_core::pipeline::prepare(raw, &plotgram_core::prepare::StyleRequest::default())
            .ok()?;
    Some(output.diagram.into_inner())
}

/// 仓库根：`CARGO_MANIFEST_DIR`（=crates/plotgram-eval）的上两级。
fn repo_root() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    Path::new(&manifest).join("../..")
}

/// 读取图集清单（相对仓库根的 .pgm 路径，跳过 `#` 注释与空行）。
fn load_set(set_path: &Path, root: &Path) -> Vec<(String, Diagram)> {
    let content = match fs::read_to_string(set_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("✗ 无法读取图集清单 {:?}: {}", set_path, e);
            std::process::exit(1);
        }
    };
    let mut diagrams = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let path = root.join(line);
        let source = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("  跳过 {}: {}", line, e);
                continue;
            }
        };
        match try_parse_diagram(&source) {
            Some(d) => {
                let name = Path::new(line)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(line)
                    .trim_end_matches(".pgm")
                    .to_string();
                diagrams.push((name, d));
            }
            None => eprintln!("  跳过 {}: 解析失败", line),
        }
    }
    diagrams
}

/// 探测单图：A1 检查 + 口径一（表达上界）+ 口径二（端口生产 + 需求画像）。
fn probe_diagram(name: &str, diagram: &Diagram) -> DiagramProbe {
    let bp = derive_channel_blueprint(diagram);
    let edges = bp.edges.clone();
    let dc_prone = matches!(
        diagram.diagram_type,
        DiagramType::Flowchart | DiagramType::Architecture
    ) && !diagram.groups.is_empty();
    // 门面已消解 flat 口径交叠组：差值即丢弃数，报告标注。
    let dropped_groups = diagram.groups.len().saturating_sub(bp.groups.len());
    // 门面已排除自环（L7-T5）：差值即自环数，报告标注。
    let self_loops = diagram.relations.len().saturating_sub(edges.len());

    let mut result = DiagramProbe {
        name: name.to_string(),
        dtype: diagram.diagram_type.clone(),
        nodes: bp.nodes.len(),
        edges: edges.len(),
        penetrations: 0,
        ub_converged: 0,
        ub_infeasible: Vec::new(),
        unrepresentable: Vec::new(),
        borrowed_passages: 0,
        gate_inconsistencies: 0,
        double_crossings: 0,
        port_converged: 0,
        port_failed: Vec::new(),
        peak_side_load: 0,
        peak_lane_demand: 0,
        peak_lane_track: None,
        peak_gate_demand: 0,
        dc_prone,
        dropped_groups,
        self_loops,
    };

    // derive 失败（空组 / 组交叠 / 未知节点等）：全部边记为不可表达。
    let (mut substrate, mut index) = match derive_substrate(&bp) {
        Ok(pair) => pair,
        Err(_) => {
            result.unrepresentable = edges.clone();
            return result;
        }
    };

    // A1：L6 组穿透检查（构建后、布线前——违规即基底非法）。
    result.penetrations = substrate.verify_no_group_penetration().len();

    // ---- 口径一：四侧候选（cap=0 不限）+ L8 掩码，每边独立空占用 ----
    if derive_node_ports(&mut substrate, &bp, &mut index, &side_port_options(0)).is_err() {
        result.unrepresentable = edges.clone();
        return result;
    }
    let graph = ChannelGraph::from_substrate(&substrate);
    let occ = Occupancy::new();
    for (from, to) in &edges {
        let froms = index.node_ports.get(from).map(Vec::as_slice).unwrap_or(&[]);
        let tos = index.node_ports.get(to).map(Vec::as_slice).unwrap_or(&[]);
        if froms.is_empty() || tos.is_empty() {
            result.unrepresentable.push((from.clone(), to.clone()));
            continue;
        }
        let mask = index.scope_mask_for_edge(&substrate, from, to);
        match route_candidates(&graph, froms, tos, &occ, &mask) {
            Ok(out) if out.status == SolverStatus::Converged => {
                result.ub_converged += 1;
                // A2/A2b 自反证：独立验证器逐轨道、逐转移断言（不复用
                // ScopeMask 的过滤代码路径，22 号文 §8：机制不自己作证）。
                let violations = verify_route_scope(
                    &substrate,
                    &out.tracks,
                    &out.gates,
                    index.node_scope(from),
                    index.node_scope(to),
                );
                if violations
                    .iter()
                    .any(|v| matches!(v, RouteScopeViolation::ForeignScope { .. }))
                {
                    result.borrowed_passages += 1;
                }
                if violations
                    .iter()
                    .any(|v| !matches!(v, RouteScopeViolation::ForeignScope { .. }))
                {
                    result.gate_inconsistencies += 1;
                }
                // A3 复核：同一 gate 不得穿越两次。
                let mut gs = out.gates.clone();
                gs.sort();
                let n = gs.len();
                gs.dedup();
                if gs.len() < n {
                    result.double_crossings += 1;
                }
            }
            _ => result.ub_infeasible.push((from.clone(), to.clone())),
        }
    }

    // ---- 口径二：共享 (node, side) 槽 cap=4，共享占用按声明序贪心 ----
    // 重新 derive 干净基底（口径一已挂 cap=0 槽，不可复用）。
    let (mut substrate2, mut index2) = match derive_substrate(&bp) {
        Ok(pair) => pair,
        Err(_) => return result,
    };
    if derive_node_ports(
        &mut substrate2,
        &bp,
        &mut index2,
        &side_port_options(PORT_SIDE_CAPACITY),
    )
    .is_err()
    {
        return result;
    }
    let graph2 = ChannelGraph::from_substrate(&substrate2);
    let mut occ2 = Occupancy::new();
    for (from, to) in &edges {
        let froms = index2.node_ports.get(from).cloned().unwrap_or_default();
        let tos = index2.node_ports.get(to).cloned().unwrap_or_default();
        if froms.is_empty() || tos.is_empty() {
            continue; // 不可表达已在口径一计数
        }
        let mask = index2.scope_mask_for_edge(&substrate2, from, to);
        // 手动候选循环（复刻 route_candidates 的升序遍历 + 严格 `<` 平局规则）：
        // 需要获胜端口对做 commit_ports；node_ports 按 id 升序分配，无需再排。
        let mut best: Option<(RouteOutcome, PortSlotId, PortSlotId)> = None;
        for &pa in &froms {
            for &pb in &tos {
                if let Ok(out) = route(&graph2, pa, pb, &occ2, &mask) {
                    if out.status == SolverStatus::Converged
                        && best.as_ref().is_none_or(|(b, _, _)| out.cost < b.cost)
                    {
                        best = Some((out, pa, pb));
                    }
                }
            }
        }
        match best {
            Some((out, pa, pb)) => {
                occ2.commit(&out.tracks, &out.gates);
                occ2.commit_ports(&[pa, pb]);
                result.port_converged += 1;
            }
            None => result.port_failed.push((from.clone(), to.clone())),
        }
    }
    // A6：峰值端口侧负载；A5：峰值 lane demand；gate crossing_demand 峰值。
    for ports in index2.node_ports.values() {
        for &pid in ports {
            result.peak_side_load = result.peak_side_load.max(occ2.port_load(pid));
        }
    }
    for t in substrate2.tracks() {
        let d = occ2.lane_demand(t.id);
        if d > result.peak_lane_demand {
            result.peak_lane_demand = d;
            result.peak_lane_track = Some(t.id);
        }
    }
    for g in substrate2.gates() {
        result.peak_gate_demand = result.peak_gate_demand.max(occ2.gate_load(g.id));
    }

    result
}

/// 口径三（附录）：gate 容量诊断扫描——`Fixed(cap)` override，四侧候选（cap=0）
/// + 共享占用贪心，返回 (成功, 总边)。量化固定容量的争用损失，不进生产结论。
fn probe_gate_diagnostic(diagram: &Diagram, cap: u32) -> Option<(usize, usize)> {
    let mut bp = derive_channel_blueprint(diagram);
    bp.gate_capacity_override = Some(cap);
    let edges = bp.edges.clone();
    let (mut substrate, mut index) = derive_substrate(&bp).ok()?;
    derive_node_ports(&mut substrate, &bp, &mut index, &side_port_options(0)).ok()?;

    let graph = ChannelGraph::from_substrate(&substrate);
    let mut occ = Occupancy::new();
    let mut converged = 0usize;
    for (from, to) in &edges {
        let froms = index.node_ports.get(from).map(Vec::as_slice).unwrap_or(&[]);
        let tos = index.node_ports.get(to).map(Vec::as_slice).unwrap_or(&[]);
        if froms.is_empty() || tos.is_empty() {
            continue;
        }
        let mask = index.scope_mask_for_edge(&substrate, from, to);
        if let Ok(out) = route_candidates(&graph, froms, tos, &occ, &mask) {
            if out.status == SolverStatus::Converged {
                occ.commit(&out.tracks, &out.gates);
                converged += 1;
            }
        }
    }
    Some((converged, edges.len()))
}

fn type_key(dt: &DiagramType) -> &'static str {
    match dt {
        DiagramType::Flowchart => "flowchart",
        DiagramType::Er => "er",
        DiagramType::State => "state",
        DiagramType::Architecture => "architecture",
        DiagramType::Sequence => "sequence",
        DiagramType::Mindmap => "mindmap",
        _ => "other",
    }
}

fn render_report(probes: &[DiagramProbe], set_name: &str) -> String {
    let total_edges: usize = probes.iter().map(|p| p.edges).sum();
    let total_ub: usize = probes.iter().map(|p| p.ub_converged).sum();
    let total_inf: usize = probes.iter().map(|p| p.ub_infeasible.len()).sum();
    let total_unrep: usize = probes.iter().map(|p| p.unrepresentable.len()).sum();
    let total_pen: usize = probes.iter().map(|p| p.penetrations).sum();
    let total_borrow: usize = probes.iter().map(|p| p.borrowed_passages).sum();
    let total_gate_inc: usize = probes.iter().map(|p| p.gate_inconsistencies).sum();
    let total_double: usize = probes.iter().map(|p| p.double_crossings).sum();
    let total_port: usize = probes.iter().map(|p| p.port_converged).sum();
    let total_port_fail: usize = probes.iter().map(|p| p.port_failed.len()).sum();
    let overall = if total_edges == 0 {
        1.0
    } else {
        total_ub as f64 / total_edges as f64
    };

    let mut out = String::new();
    out.push_str("# Atlas channel 合法化探针报告（27 号文 §5.1）\n\n");
    out.push_str(&format!("- 图集：`{}`\n", set_name));
    out.push_str(&format!("- 图数：{}\n", probes.len()));
    out.push_str("- 基底：段模型 derive（L1 切割 + L2 配对 gate + L3 span_weight），gate 生产恒 `Unbounded`（L4）\n");
    out.push_str(&format!(
        "- 端口侧容量（口径二）：{}\n\n",
        PORT_SIDE_CAPACITY
    ));

    out.push_str("## 总体（合法性红线 A1–A3 + 表达上界 A4）\n\n");
    out.push_str(&format!(
        "| 指标 | 值 | 预期 |\n|---|---|---|\n\
         | A1 组穿透违规（L6） | {} | 0 |\n\
         | A2 借道穿组边数（路径自反证） | {} | 0 |\n\
         | A2b gate 一致性违规边数（路径自反证） | {} | 0 |\n\
         | A3 同 gate 双穿边数（路径复核） | {} | 0 |\n\
         | A4 合法单边可行率（表达上界） | {}/{} = {:.1}% | — |\n\n",
        total_pen,
        total_borrow,
        total_gate_inc,
        total_double,
        total_ub,
        total_edges,
        overall * 100.0
    ));
    out.push_str(&format!(
        "- 口径一失败构成：选路不可行 {} · 网格不可表达 {}\n",
        total_inf, total_unrep
    ));
    out.push_str(&format!(
        "- 口径二（端口生产）：成功 {} · 端口/争用失败 {}\n\n",
        total_port, total_port_fail
    ));

    // 按图类型分组（口径一）
    let mut by_type: BTreeMap<&'static str, (usize, usize, usize, usize)> = BTreeMap::new();
    for p in probes {
        let e = by_type.entry(type_key(&p.dtype)).or_insert((0, 0, 0, 0));
        e.0 += p.edges;
        e.1 += p.ub_converged;
        e.2 += p.ub_infeasible.len();
        e.3 += p.unrepresentable.len();
    }
    out.push_str("## 口径一：表达上界，按图类型\n\n");
    out.push_str("四侧候选端口（cap=0 不限）+ gate `Unbounded` + L8 作用域掩码，每边独立空占用。\n\n");
    out.push_str("| 类型 | 边数 | 成功 | 不可行 | 不可表达 | 成功率 |\n");
    out.push_str("|---|---|---|---|---|---|\n");
    for (t, (edges, ok, inf, unrep)) in &by_type {
        let rate = if *edges == 0 {
            1.0
        } else {
            *ok as f64 / *edges as f64
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {:.1}% |\n",
            t,
            edges,
            ok,
            inf,
            unrep,
            rate * 100.0
        ));
    }
    out.push('\n');

    // 逐图明细：三口径并列
    out.push_str("## 逐图明细（三口径并列）\n\n");
    out.push_str("| 图 | 类型 | 节点 | 边 | A1 | 上界成功 | A2 | A2b | A3 | 端口成功 | 争用失败 | A6峰值侧负载 | A5峰值lane | 峰值gate需求 | 标注 |\n");
    out.push_str("|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|\n");
    for p in probes {
        let mut notes: Vec<String> = Vec::new();
        if p.dc_prone {
            notes.push("divide-conquer/two_phase".to_string());
        }
        if p.dropped_groups > 0 {
            notes.push(format!("丢交叠组×{}", p.dropped_groups));
        }
        if p.self_loops > 0 {
            notes.push(format!("排自环×{}", p.self_loops));
        }
        let note = notes.join(" · ");
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} ({:.0}%) | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            p.name,
            type_key(&p.dtype),
            p.nodes,
            p.edges,
            p.penetrations,
            p.ub_converged,
            p.ub_rate() * 100.0,
            p.borrowed_passages,
            p.gate_inconsistencies,
            p.double_crossings,
            p.port_converged,
            p.port_failed.len(),
            p.peak_side_load,
            p.peak_lane_demand,
            p.peak_gate_demand,
            note
        ));
    }
    out.push('\n');
    out.push_str(
        "（A5 峰值 lane demand 基线参照 25 号文旧网格采样：product 10 / stress 33 / demo 25）\n\n",
    );
    let total_dropped: usize = probes.iter().map(|p| p.dropped_groups).sum();
    if total_dropped > 0 {
        out.push_str(&format!(
            "（flat 口径消解：共丢弃 {} 个包围盒交叠组——LayeredKernel 不做组感知布局，\
             兄弟组矩形交叠破坏切割模型嵌套树前提；受影响图的 A2 检验力度打折，\
             见明细「标注」列。生产接线将镜像真实分区布局，不受此限。）\n\n",
            total_dropped
        ));
    }
    let total_self_loops: usize = probes.iter().map(|p| p.self_loops).sum();
    if total_self_loops > 0 {
        out.push_str(&format!(
            "（自环排除：共 {} 条同节点边不计入采样——channel 不建模自环（L7-T5，\
             route 显式 Infeasible），生产管线自环走节点旁小环、不经通道；\
             受影响图见明细「标注」列。）\n\n",
            total_self_loops
        ));
    }

    // 失败边清单
    let has_fail = probes.iter().any(|p| !p.ub_infeasible.is_empty());
    let has_unrep = probes.iter().any(|p| !p.unrepresentable.is_empty());
    let has_port_fail = probes.iter().any(|p| !p.port_failed.is_empty());
    if has_fail {
        out.push_str("## 口径一选路不可行边（Infeasible）\n\n");
        for p in probes {
            if p.ub_infeasible.is_empty() {
                continue;
            }
            out.push_str(&format!("- **{}**：\n", p.name));
            for (f, t) in &p.ub_infeasible {
                out.push_str(&format!("  - {} → {}\n", f, t));
            }
        }
        out.push('\n');
    }
    if has_unrep {
        out.push_str("## 网格不可表达边（端点落不到轨道）\n\n");
        for p in probes {
            if p.unrepresentable.is_empty() {
                continue;
            }
            out.push_str(&format!("- **{}**：\n", p.name));
            for (f, t) in &p.unrepresentable {
                out.push_str(&format!("  - {} → {}\n", f, t));
            }
        }
        out.push('\n');
    }
    if has_port_fail {
        out.push_str("## 口径二端口/争用失败边\n\n");
        for p in probes {
            if p.port_failed.is_empty() {
                continue;
            }
            out.push_str(&format!("- **{}**：\n", p.name));
            for (f, t) in &p.port_failed {
                out.push_str(&format!("  - {} → {}\n", f, t));
            }
        }
        out.push('\n');
    }
    if !has_fail && !has_unrep && !has_port_fail {
        out.push_str("_无失败边：口径一/二全部边选路成功。_\n");
    }

    out
}

/// 附录：gate 容量诊断扫描（`Fixed(cap)` override，不进生产结论）。
fn render_gate_scan(diagrams: &[(String, Diagram)]) -> String {
    let mut out = String::new();
    out.push_str("\n## 附录：gate 容量诊断扫描（Fixed override）\n\n");
    out.push_str(&format!(
        "口径：`gate_capacity_override = Some(cap)`，四侧候选端口（cap=0）+ 共享占用声明序贪心。\
         生产恒 `Unbounded`，本表仅量化固定容量的争用损失（诊断基准 cap={}）。\n\n",
        plotgram_core::layout::atlas::probe::DEFAULT_GATE_CAPACITY
    ));
    out.push_str("| gate 容量 | 争用成功 | 总边 | 成功率 |\n|---|---|---|---|\n");
    for &cap in &[1u32, 2, 4, 8] {
        let mut ok = 0usize;
        let mut edges = 0usize;
        for (_, diagram) in diagrams {
            if let Some((c, e)) = probe_gate_diagnostic(diagram, cap) {
                ok += c;
                edges += e;
            }
        }
        out.push_str(&format!(
            "| {} | {} | {} | {:.1}% |\n",
            cap,
            ok,
            edges,
            if edges == 0 {
                100.0
            } else {
                ok as f64 / edges as f64 * 100.0
            }
        ));
    }
    out.push('\n');
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let root = repo_root();

    let set_path = args
        .iter()
        .position(|a| a == "--set")
        .and_then(|i| args.get(i + 1).cloned())
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("benchmarks/sets/product-regression-set.txt"));

    let output_path = args
        .iter()
        .position(|a| a == "--output" || a == "-o")
        .and_then(|i| args.get(i + 1).cloned());

    if !set_path.exists() {
        eprintln!("✗ 图集清单不存在: {:?}", set_path);
        eprintln!("  用 --set <file> 指定");
        std::process::exit(1);
    }

    eprintln!("▶ 图集清单: {:?}", set_path);
    let diagrams = load_set(&set_path, &root);
    eprintln!("  加载 {} 个图", diagrams.len());
    if diagrams.is_empty() {
        eprintln!("✗ 没有可探测的图");
        std::process::exit(1);
    }

    let mut probes = Vec::with_capacity(diagrams.len());
    for (name, diagram) in &diagrams {
        let p = probe_diagram(name, diagram);
        eprintln!(
            "  {:<48} 边{:>3}  上界{:>3} {:.0}%  A1={} A2={} A2b={} A3={}  端口{:>3}{}",
            p.name,
            p.edges,
            p.ub_converged,
            p.ub_rate() * 100.0,
            p.penetrations,
            p.borrowed_passages,
            p.gate_inconsistencies,
            p.double_crossings,
            p.port_converged,
            if p.dc_prone { "  [D&C]" } else { "" }
        );
        probes.push(p);
    }

    let set_name = set_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("custom");
    let mut report = render_report(&probes, set_name);
    report.push_str(&render_gate_scan(&diagrams));

    match output_path {
        Some(path) => {
            fs::write(&path, &report).expect("写入输出文件失败");
            eprintln!("\n报告已写入 {}", path);
        }
        None => {
            println!("{}", report);
        }
    }
}
