//! Atlas 通道图性能探针（相 I 全链路计时）。
//!
//! 对图集里每张图计时四段：
//!   1. blueprint  —— `derive_channel_blueprint`（含 `LayeredKernel::compute` 求 rank/order）
//!   2. substrate  —— `derive_substrate`（铺 track/link/gate + 自动挂端口）
//!   3. graph      —— `ChannelGraph::from_substrate`（构建确定性邻接表）
//!   4. route-all  —— 共享 `Occupancy` 按声明序路完全部边并提交占用（生产口径）
//!
//! 每段跑 `ITERS` 次取**中位数**，汇总每集总耗时与单边均摊。仅本 bin 属性能测量，
//! 按 AGENTS.md §9 允许 `--release` 运行：
//!   cargo run --release -p plotgram-eval --bin atlas_perf
//!   cargo run --release -p plotgram-eval --bin atlas_perf -- --set benchmarks/sets/demo-observe-set.txt
//!   cargo run --release -p plotgram-eval --bin atlas_perf -- --output /tmp/atlas_perf.md

use plotgram_core::ast::Diagram;
use plotgram_core::layout::atlas::channel::{
    derive_substrate, route, ChannelGraph, Occupancy, PortSide, PortSlotId, ScopeMask, Substrate,
    TrackId,
};
use plotgram_core::layout::atlas::probe::derive_channel_blueprint;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const ITERS: usize = 50;

/// 探针挂接辅助（与 atlas_probe 相同）：按宿主轨道走向选相容侧，
/// `slot_counters` 按 `(node, side)` 递增分配 `slot_index`（P-inv-2 平行边建模）。
fn attach_probe_port(
    substrate: &mut Substrate,
    slot_counters: &mut BTreeMap<(String, PortSide), u32>,
    id: PortSlotId,
    node: &str,
    track: TrackId,
) -> Result<(), plotgram_core::layout::atlas::channel::SubstrateError> {
    let side = match substrate.track(track).map(|t| t.orient) {
        Some(plotgram_core::layout::atlas::channel::TrackOrient::Cross) => PortSide::MainLow,
        _ => PortSide::CrossLow,
    };
    let slot_index = {
        let counter = slot_counters.entry((node.to_string(), side)).or_insert(0);
        let idx = *counter;
        *counter += 1;
        idx
    };
    substrate.attach_port(id, node, side, slot_index, track, 0)
}

fn try_parse_diagram(source: &str) -> Option<Diagram> {
    let raw = plotgram_core::pipeline::parse(source).ok()?;
    let output =
        plotgram_core::pipeline::prepare(raw, &plotgram_core::prepare::StyleRequest::default())
            .ok()?;
    Some(output.diagram.into_inner())
}

fn repo_root() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    Path::new(&manifest).join("../..")
}

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

/// 跑 `ITERS` 次取中位数（微秒）。
fn median_micros(mut f: impl FnMut()) -> f64 {
    let mut samples = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        let t = Instant::now();
        f();
        samples.push(t.elapsed().as_nanos() as f64 / 1000.0);
    }
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
    samples[samples.len() / 2]
}

struct PerfRow {
    name: String,
    nodes: usize,
    edges: usize,
    tracks: usize,
    gates: usize,
    transitions: usize,
    blueprint_us: f64,
    substrate_us: f64,
    graph_us: f64,
    route_all_us: f64,
}

impl PerfRow {
    fn total_us(&self) -> f64 {
        self.blueprint_us + self.substrate_us + self.graph_us + self.route_all_us
    }
    fn per_edge_us(&self) -> f64 {
        if self.edges == 0 {
            0.0
        } else {
            self.route_all_us / self.edges as f64
        }
    }
}

/// 单图四段计时（每段独立取中位数）。
fn perf_diagram(name: &str, diagram: &Diagram) -> Option<PerfRow> {
    // 1) blueprint（含 LayeredKernel::compute）
    let blueprint_us = median_micros(|| {
        let bp = derive_channel_blueprint(diagram);
        std::hint::black_box(&bp);
    });
    let bp = derive_channel_blueprint(diagram);

    // 2) substrate（derive：铺轨道/gate + 自动挂端口）
    let substrate_us = median_micros(|| {
        let pair = derive_substrate(&bp);
        std::hint::black_box(&pair);
    });
    let (mut substrate, index) = derive_substrate(&bp).ok()?;

    // 探针端口挂接（一次性，非生产计时段，与 atlas_probe 口径一致）；
    // 每边的 L8 作用域掩码也在计时环外预构建（生产中掩码随边给定）。
    let edges = bp.edges.clone();
    let mut port_pairs: Vec<Option<(PortSlotId, PortSlotId, ScopeMask)>> =
        Vec::with_capacity(edges.len());
    let mut slot_counters: BTreeMap<(String, PortSide), u32> = BTreeMap::new();
    for (from, to) in &edges {
        match index.edge_port_tracks(&bp, from, to) {
            Some((src_track, dst_track)) => {
                let pa = substrate.alloc_port_id();
                if attach_probe_port(&mut substrate, &mut slot_counters, pa, from, src_track)
                    .is_err()
                {
                    port_pairs.push(None);
                    continue;
                }
                let pb = substrate.alloc_port_id();
                if attach_probe_port(&mut substrate, &mut slot_counters, pb, to, dst_track)
                    .is_err()
                {
                    port_pairs.push(None);
                    continue;
                }
                let mask = index.scope_mask_for_edge(&substrate, from, to);
                port_pairs.push(Some((pa, pb, mask)));
            }
            None => port_pairs.push(None),
        }
    }

    // 3) graph（确定性邻接表构建）
    let graph_us = median_micros(|| {
        let g = ChannelGraph::from_substrate(&substrate);
        std::hint::black_box(&g);
    });
    let graph = ChannelGraph::from_substrate(&substrate);

    // 4) route-all（共享占用、声明序贪心提交——生产口径，含 L8 掩码）
    let route_all_us = median_micros(|| {
        let mut occ = Occupancy::new();
        for pair in &port_pairs {
            if let Some((pa, pb, mask)) = pair {
                if let Ok(out) = route(&graph, *pa, *pb, &occ, mask) {
                    occ.commit(&out.tracks, &out.gates);
                    occ.commit_ports(&[*pa, *pb]);
                }
            }
        }
        std::hint::black_box(&occ);
    });

    let tracks = substrate.tracks().count();
    let gates = substrate.gates().count();
    let transitions: usize = substrate
        .tracks()
        .map(|t| graph.neighbors(t.id).len())
        .sum();

    Some(PerfRow {
        name: name.to_string(),
        nodes: bp.nodes.len(),
        edges: edges.len(),
        tracks,
        gates,
        transitions,
        blueprint_us,
        substrate_us,
        graph_us,
        route_all_us,
    })
}

fn render_report(rows: &[PerfRow], set_name: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Atlas 通道图性能探针（{}）\n\n", set_name));
    out.push_str(&format!(
        "- 图数：{} · 每段 {} 次迭代取中位数 · release 构建\n",
        rows.len(),
        ITERS
    ));
    out.push_str("- 段定义：blueprint=`derive_channel_blueprint`（含分层求解）；substrate=`derive_substrate`；graph=`ChannelGraph::from_substrate`；route-all=共享占用声明序路完全部边并提交\n\n");

    let sum = |f: fn(&PerfRow) -> f64| rows.iter().map(f).sum::<f64>();
    let total_edges: usize = rows.iter().map(|r| r.edges).sum();
    let total_route = sum(|r| r.route_all_us);
    let total_all = sum(PerfRow::total_us);
    out.push_str("## 汇总\n\n");
    out.push_str("| 段 | 全集总耗时 | 占比 |\n|---|---|---|\n");
    for (label, v) in [
        ("blueprint", sum(|r| r.blueprint_us)),
        ("substrate", sum(|r| r.substrate_us)),
        ("graph", sum(|r| r.graph_us)),
        ("route-all", total_route),
    ] {
        out.push_str(&format!(
            "| {} | {:.1} µs | {:.1}% |\n",
            label,
            v,
            v / total_all * 100.0
        ));
    }
    out.push_str(&format!(
        "| **合计** | **{:.1} µs = {:.2} ms** | 100% |\n\n",
        total_all,
        total_all / 1000.0
    ));
    out.push_str(&format!(
        "- 总边数 {} · route-all 单边均摊 **{:.2} µs/边** · 全链路单图均摊 **{:.1} µs/图**\n\n",
        total_edges,
        total_route / total_edges.max(1) as f64,
        total_all / rows.len().max(1) as f64
    ));

    out.push_str("## 逐图明细（按全链路耗时降序）\n\n");
    out.push_str("| 图 | 节点 | 边 | track | gate | 转移 | blueprint | substrate | graph | route-all | 合计 | µs/边 |\n");
    out.push_str("|---|---|---|---|---|---|---|---|---|---|---|---|\n");
    let mut sorted: Vec<&PerfRow> = rows.iter().collect();
    sorted.sort_by(|a, b| b.total_us().partial_cmp(&a.total_us()).unwrap());
    for r in sorted {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {:.1} | {:.1} | {:.1} | {:.1} | {:.1} | {:.2} |\n",
            r.name,
            r.nodes,
            r.edges,
            r.tracks,
            r.gates,
            r.transitions,
            r.blueprint_us,
            r.substrate_us,
            r.graph_us,
            r.route_all_us,
            r.total_us(),
            r.per_edge_us()
        ));
    }
    out.push_str("\n（时间单位均为 µs，中位数）\n");
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
        std::process::exit(1);
    }

    eprintln!("▶ 图集清单: {:?}（每段 {} 次迭代取中位数）", set_path, ITERS);
    let diagrams = load_set(&set_path, &root);
    eprintln!("  加载 {} 个图", diagrams.len());

    let mut rows = Vec::with_capacity(diagrams.len());
    for (name, diagram) in &diagrams {
        match perf_diagram(name, diagram) {
            Some(r) => {
                eprintln!(
                    "  {:<48} 边{:>3}  全链路 {:>8.1}µs  route {:>7.1}µs",
                    r.name,
                    r.edges,
                    r.total_us(),
                    r.route_all_us
                );
                rows.push(r);
            }
            None => eprintln!("  {:<48} derive 失败，跳过", name),
        }
    }

    let set_name = set_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("custom");
    let report = render_report(&rows, set_name);

    match output_path {
        Some(p) => {
            fs::write(&p, &report).expect("写报告失败");
            eprintln!("✓ 报告已写入 {}", p);
        }
        None => println!("{}", report),
    }
}
