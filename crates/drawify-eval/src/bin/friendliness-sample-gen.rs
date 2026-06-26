//! Phase 1.5 压力样本生成器
//!
//! 生成多样化的合成 .dfy 图文件到指定目录（默认 benchmarks/friendliness_stress），
//! 用于扩样至 500+ 以做相关性校准。不写入 showcase 目录。
//!
//! 生成的拓扑：
//! - chain（线性链）/ grid（网格）/ star（星形）/ bipartite（二部图）
//! - tree（平衡树）/ dag（分层 DAG）/ multigroup（多 group）/ hublayer（多层 hub）
//!
//! 用法:
//!   cargo run -p drawify-eval --bin friendliness-sample-gen -- /path/to/friendliness_stress

use std::fs;
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out_dir = args
        .iter()
        .skip(1)
        .next()
        .cloned()
        .unwrap_or_else(|| {
            let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
            format!("{}/../../../benchmarks/friendliness_stress", manifest_dir)
        });

    let out_path = Path::new(&out_dir);
    fs::create_dir_all(out_path).expect("无法创建输出目录");

    let mut count = 0usize;
    let mut buf = String::new();

    // ── 1. Chain（线性链）：长边少，crossing 少 ──
    for &n in &[6usize, 8, 10, 12, 16, 20, 24, 30] {
        buf.clear();
        gen_chain(&mut buf, n);
        write_file(out_path, &format!("chain-n{}.dfy", n), &buf);
        count += 1;
    }

    // ── 2. Grid（网格）：通道占用度高 ──
    for &(r, c) in &[(3usize, 3usize), (3, 4), (4, 4), (4, 5), (5, 5), (5, 6), (6, 6)] {
        buf.clear();
        gen_grid(&mut buf, r, c);
        write_file(out_path, &format!("grid-{}x{}.dfy", r, c), &buf);
        count += 1;
    }

    // ── 3. Star（星形）：端口冲突高 ──
    for &n in &[6usize, 8, 10, 12, 16, 20, 24] {
        buf.clear();
        gen_star(&mut buf, n);
        write_file(out_path, &format!("star-n{}.dfy", n), &buf);
        count += 1;
    }

    // ── 4. Bipartite（二部图）：crossing 高 ──
    for &(l, r) in &[(3usize, 3usize), (3, 4), (4, 4), (4, 5), (5, 5), (5, 6), (6, 6)] {
        buf.clear();
        gen_bipartite(&mut buf, l, r);
        write_file(out_path, &format!("bipartite-{}x{}.dfy", l, r), &buf);
        count += 1;
    }

    // ── 5. Tree（平衡树）：层次结构 ──
    for &(depth, branch) in &[(2usize, 3usize), (3, 2), (3, 3), (3, 4), (4, 2), (4, 3)] {
        buf.clear();
        if let Some(s) = gen_tree(&mut buf, depth, branch) {
            write_file(out_path, &format!("tree-d{}b{}.dfy", depth, branch), &buf);
            count += 1;
            let _ = s;
        }
    }

    // ── 6. DAG（分层随机 DAG）──
    for &layers in &[3usize, 4, 5, 6] {
        for &per_layer in &[3usize, 4, 5] {
            buf.clear();
            gen_dag(&mut buf, layers, per_layer, 42);
            write_file(out_path, &format!("dag-L{}P{}.dfy", layers, per_layer), &buf);
            count += 1;
        }
    }

    // ── 7. Multi-group（多 group，architecture 类型）──
    for &ng in &[3usize, 4, 5, 6] {
        buf.clear();
        gen_multigroup(&mut buf, ng);
        write_file(out_path, &format!("multigroup-g{}.dfy", ng), &buf);
        count += 1;
    }

    // ── 8. Hub-layer（多层 hub-spoke）──
    for &layers in &[2usize, 3, 4] {
        for &spokes in &[3usize, 4, 5] {
            buf.clear();
            gen_hublayer(&mut buf, layers, spokes);
            write_file(out_path, &format!("hublayer-L{}S{}.dfy", layers, spokes), &buf);
            count += 1;
        }
    }

    // ── 9. Dense flowchart（高边密度）──
    for &n in &[10usize, 14, 18, 22, 26] {
        buf.clear();
        gen_dense_flow(&mut buf, n);
        write_file(out_path, &format!("dense-n{}.dfy", n), &buf);
        count += 1;
    }

    // ── 10. Wide-layer（宽层，每层多节点，层间全连接）──
    for &(layers, per_layer) in &[(2usize, 4usize), (2, 5), (2, 6), (3, 4), (3, 5), (4, 3)] {
        buf.clear();
        gen_wide_layer(&mut buf, layers, per_layer);
        write_file(out_path, &format!("wide-L{}P{}.dfy", layers, per_layer), &buf);
        count += 1;
    }

    // ── 11. State machine（state 类型，circular 适用）──
    for &n in &[5usize, 6, 7, 8, 10, 12] {
        buf.clear();
        gen_state_cycle(&mut buf, n);
        write_file(out_path, &format!("state-cycle-n{}.dfy", n), &buf);
        count += 1;
    }

    // ── 12. ER schema（er 类型）──
    for &n in &[3usize, 4, 5, 6, 8] {
        buf.clear();
        gen_er_schema(&mut buf, n);
        write_file(out_path, &format!("er-schema-n{}.dfy", n), &buf);
        count += 1;
    }

    // ── 13. 第二批：更多小中型变体（避免超时，保证 5 布局均产出样本）──
    // Ring（环形）
    for &n in &[5usize, 6, 7, 8, 10, 12, 14] {
        buf.clear();
        gen_ring(&mut buf, n);
        write_file(out_path, &format!("ring-n{}.dfy", n), &buf);
        count += 1;
    }
    // Path-with-shortcuts（带捷径的路径）
    for &n in &[8usize, 10, 12, 14, 16, 20] {
        buf.clear();
        gen_path_shortcuts(&mut buf, n);
        write_file(out_path, &format!("pathsc-n{}.dfy", n), &buf);
        count += 1;
    }
    // 更多 chain 变体
    for &n in &[4usize, 5, 7, 9, 14, 18, 22, 28] {
        buf.clear();
        gen_chain(&mut buf, n);
        write_file(out_path, &format!("chain2-n{}.dfy", n), &buf);
        count += 1;
    }
    // 更多 star 变体
    for &n in &[4usize, 5, 7, 9, 14, 18, 22] {
        buf.clear();
        gen_star(&mut buf, n);
        write_file(out_path, &format!("star2-n{}.dfy", n), &buf);
        count += 1;
    }
    // 更多 bipartite 变体
    for &(l, r) in &[(2usize, 3usize), (2, 4), (3, 3), (4, 3), (3, 5), (4, 4), (5, 4)] {
        buf.clear();
        gen_bipartite(&mut buf, l, r);
        write_file(out_path, &format!("bipartite2-{}x{}.dfy", l, r), &buf);
        count += 1;
    }
    // 更多 grid 变体
    for &(r, c) in &[(2usize, 3usize), (2, 4), (3, 3), (3, 5), (4, 3), (4, 4), (5, 4)] {
        buf.clear();
        gen_grid(&mut buf, r, c);
        write_file(out_path, &format!("grid2-{}x{}.dfy", r, c), &buf);
        count += 1;
    }
    // 更多 DAG 变体（不同 seed）
    for &layers in &[3usize, 4, 5] {
        for &per_layer in &[2usize, 3, 4] {
            buf.clear();
            gen_dag(&mut buf, layers, per_layer, 99);
            write_file(out_path, &format!("dag2-L{}P{}.dfy", layers, per_layer), &buf);
            count += 1;
        }
    }
    // 更多 dense 变体
    for &n in &[6usize, 8, 10, 12, 16, 20] {
        buf.clear();
        gen_dense_flow(&mut buf, n);
        write_file(out_path, &format!("dense2-n{}.dfy", n), &buf);
        count += 1;
    }
    // 更多 state-cycle
    for &n in &[4usize, 6, 9, 11, 14] {
        buf.clear();
        gen_state_cycle(&mut buf, n);
        write_file(out_path, &format!("state-cycle2-n{}.dfy", n), &buf);
        count += 1;
    }
    // 更多 er-schema
    for &n in &[2usize, 4, 6, 7, 9, 10] {
        buf.clear();
        gen_er_schema(&mut buf, n);
        write_file(out_path, &format!("er-schema2-n{}.dfy", n), &buf);
        count += 1;
    }
    // 双 hub 星形（两个中心节点）
    for &n in &[6usize, 8, 10, 12, 16] {
        buf.clear();
        gen_dual_hub(&mut buf, n);
        write_file(out_path, &format!("dualhub-n{}.dfy", n), &buf);
        count += 1;
    }
    // 层间稀疏连接
    for &(layers, per_layer) in &[(3usize, 3usize), (4, 2), (3, 4), (5, 2), (4, 3)] {
        buf.clear();
        gen_sparse_layer(&mut buf, layers, per_layer);
        write_file(out_path, &format!("sparse-L{}P{}.dfy", layers, per_layer), &buf);
        count += 1;
    }

    eprintln!("✓ 生成 {} 个 .dfy 文件到 {}", count, out_dir);
}

fn write_file(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    fs::write(&path, content).unwrap_or_else(|e| eprintln!("✗ 写入 {} 失败: {}", path.display(), e));
}

fn gen_chain(buf: &mut String, n: usize) {
    buf.push_str(&format!("diagram flowchart \"chain-{}\" {{\n\n", n));
    for i in 0..n {
        let t = if i == 0 { "start" } else if i == n - 1 { "end" } else { "process" };
        buf.push_str(&format!("    entity n{} \"N{}\" {{ type: {} }}\n", i, i, t));
    }
    buf.push_str("\n");
    for i in 0..n.saturating_sub(1) {
        buf.push_str(&format!("    n{} -> n{}\n", i, i + 1));
    }
    buf.push_str("}\n");
}

fn gen_grid(buf: &mut String, rows: usize, cols: usize) {
    buf.push_str(&format!("diagram flowchart \"grid-{}x{}\" {{\n\n", rows, cols));
    for r in 0..rows {
        for c in 0..cols {
            let id = r * cols + c;
            buf.push_str(&format!("    entity n{} \"{}\" {{ type: process }}\n", id, id));
        }
    }
    buf.push_str("\n");
    // 水平边
    for r in 0..rows {
        for c in 0..cols.saturating_sub(1) {
            let a = r * cols + c;
            let b = r * cols + c + 1;
            buf.push_str(&format!("    n{} -> n{}\n", a, b));
        }
    }
    // 垂直边
    for r in 0..rows.saturating_sub(1) {
        for c in 0..cols {
            let a = r * cols + c;
            let b = (r + 1) * cols + c;
            buf.push_str(&format!("    n{} -> n{}\n", a, b));
        }
    }
    buf.push_str("}\n");
}

fn gen_star(buf: &mut String, n: usize) {
    buf.push_str(&format!("diagram flowchart \"star-{}\" {{\n\n", n));
    buf.push_str("    entity hub \"Hub\" { type: process }\n");
    for i in 0..n {
        buf.push_str(&format!("    entity s{} \"S{}\" {{ type: process }}\n", i, i));
    }
    buf.push_str("\n");
    for i in 0..n {
        buf.push_str(&format!("    hub -> s{}\n", i));
    }
    buf.push_str("}\n");
}

fn gen_bipartite(buf: &mut String, l: usize, r: usize) {
    buf.push_str(&format!("diagram flowchart \"bipartite-{}x{}\" {{\n\n", l, r));
    for i in 0..l {
        buf.push_str(&format!("    entity l{} \"L{}\" {{ type: process }}\n", i, i));
    }
    for i in 0..r {
        buf.push_str(&format!("    entity r{} \"R{}\" {{ type: process }}\n", i, i));
    }
    buf.push_str("\n");
    // 全连接
    for i in 0..l {
        for j in 0..r {
            buf.push_str(&format!("    l{} -> r{}\n", i, j));
        }
    }
    buf.push_str("}\n");
}

fn gen_tree(buf: &mut String, depth: usize, branch: usize) -> Option<()> {
    // 计算节点总数
    let mut total = 0usize;
    for d in 0..=depth {
        total += branch.pow(d as u32);
    }
    if total > 60 {
        return None; // 太大跳过
    }
    buf.push_str(&format!("diagram flowchart \"tree-d{}b{}\" {{\n\n", depth, branch));
    // 节点：按层编号，root=0
    let mut id = 0usize;
    let mut layer_starts: Vec<usize> = vec![0];
    for d in 0..=depth {
        let start = id;
        for _ in 0..branch.pow(d as u32) {
            buf.push_str(&format!("    entity n{} \"N{}\" {{ type: process }}\n", id, id));
            id += 1;
        }
        layer_starts.push(start);
    }
    buf.push_str("\n");
    // 边：每个非叶节点连 branch 个子节点
    for d in 0..depth {
        let parent_start = layer_starts[d];
        let parent_count = branch.pow(d as u32);
        let child_start = layer_starts[d + 1];
        for p in 0..parent_count {
            for b in 0..branch {
                let parent_id = parent_start + p;
                let child_id = child_start + p * branch + b;
                buf.push_str(&format!("    n{} -> n{}\n", parent_id, child_id));
            }
        }
    }
    buf.push_str("}\n");
    Some(())
}

fn gen_dag(buf: &mut String, layers: usize, per_layer: usize, seed: u64) {
    buf.push_str(&format!("diagram flowchart \"dag-L{}P{}\" {{\n\n", layers, per_layer));
    let mut id = 0usize;
    let mut layer_nodes: Vec<Vec<usize>> = Vec::new();
    for l in 0..layers {
        let mut layer = Vec::new();
        for _ in 0..per_layer {
            buf.push_str(&format!("    entity n{} \"N{}\" {{ type: process }}\n", id, id));
            layer.push(id);
            id += 1;
        }
        layer_nodes.push(layer);
    }
    buf.push_str("\n");
    // 简单 LCG 伪随机
    let mut s = seed;
    let mut rng = || {
        s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        s
    };
    // 层间边：每层每个节点连下一层 1-2 个节点
    for l in 0..layers.saturating_sub(1) {
        for &node in &layer_nodes[l] {
            let next = &layer_nodes[l + 1];
            // 连 1-2 个
            let n_conn = 1 + (rng() % 2) as usize;
            for k in 0..n_conn {
                let target = next[(rng() as usize) % next.len()];
                buf.push_str(&format!("    n{} -> n{}\n", node, target));
            }
        }
    }
    buf.push_str("}\n");
}

fn gen_multigroup(buf: &mut String, ng: usize) {
    buf.push_str(&format!("diagram architecture \"multigroup-g{}\" {{\n\n", ng));
    for g in 0..ng {
        buf.push_str(&format!("    group g{} \"Group {}\" {{\n", g, g));
        for i in 0..3 {
            buf.push_str(&format!("        entity n{}_{} \"N{}-{}\" {{ type: service }}\n", g, i, g, i));
        }
        buf.push_str("    }\n");
    }
    buf.push_str("\n");
    // 组内边
    for g in 0..ng {
        buf.push_str(&format!("    n{}_0 -> n{}_1\n", g, g));
        buf.push_str(&format!("    n{}_1 -> n{}_2\n", g, g));
    }
    // 组间边：链式 + 跨组
    for g in 0..ng.saturating_sub(1) {
        buf.push_str(&format!("    n{}_2 -> n{}_0\n", g, g + 1));
    }
    // 一些跨组边
    if ng >= 4 {
        buf.push_str(&format!("    n{}_2 -> n{}_0\n", 0, ng / 2));
        buf.push_str(&format!("    n{}_2 -> n{}_0\n", ng / 2, 0));
    }
    buf.push_str("}\n");
}

fn gen_hublayer(buf: &mut String, layers: usize, spokes: usize) {
    buf.push_str(&format!("diagram flowchart \"hublayer-L{}S{}\" {{\n\n", layers, spokes));
    let mut hubs = Vec::new();
    let mut id = 0usize;
    for l in 0..layers {
        buf.push_str(&format!("    entity h{} \"Hub{}\" {{ type: process }}\n", id, l));
        hubs.push(id);
        id += 1;
        for _ in 0..spokes {
            buf.push_str(&format!("    entity n{} \"N{}\" {{ type: process }}\n", id, id));
            buf.push_str(&format!("    h{} -> n{}\n", hubs[l], id));
            id += 1;
        }
    }
    buf.push_str("\n");
    // hub 之间链式连接
    for l in 0..layers.saturating_sub(1) {
        buf.push_str(&format!("    h{} -> h{}\n", hubs[l], hubs[l + 1]));
    }
    buf.push_str("}\n");
}

fn gen_dense_flow(buf: &mut String, n: usize) {
    buf.push_str(&format!("diagram flowchart \"dense-n{}\" {{\n\n", n));
    for i in 0..n {
        let t = if i == 0 { "start" } else if i == n - 1 { "end" } else { "process" };
        buf.push_str(&format!("    entity n{} \"N{}\" {{ type: {} }}\n", i, i, t));
    }
    buf.push_str("\n");
    // 每个节点连后续 2-3 个节点（dense）
    for i in 0..n {
        let max_j = (i + 4).min(n);
        for j in (i + 1)..max_j {
            buf.push_str(&format!("    n{} -> n{}\n", i, j));
        }
    }
    buf.push_str("}\n");
}

fn gen_wide_layer(buf: &mut String, layers: usize, per_layer: usize) {
    buf.push_str(&format!("diagram flowchart \"wide-L{}P{}\" {{\n\n", layers, per_layer));
    let mut id = 0usize;
    let mut layer_nodes: Vec<Vec<usize>> = Vec::new();
    for l in 0..layers {
        let mut layer = Vec::new();
        for _ in 0..per_layer {
            buf.push_str(&format!("    entity n{} \"N{}\" {{ type: process }}\n", id, id));
            layer.push(id);
            id += 1;
        }
        layer_nodes.push(layer);
    }
    buf.push_str("\n");
    // 层间全连接
    for l in 0..layers.saturating_sub(1) {
        for &a in &layer_nodes[l] {
            for &b in &layer_nodes[l + 1] {
                buf.push_str(&format!("    n{} -> n{}\n", a, b));
            }
        }
    }
    buf.push_str("}\n");
}

fn gen_state_cycle(buf: &mut String, n: usize) {
    buf.push_str(&format!("diagram state \"state-cycle-{}\" {{\n\n", n));
    for i in 0..n {
        buf.push_str(&format!("    entity s{} \"S{}\" {{ type: state }}\n", i, i));
    }
    buf.push_str("\n");
    for i in 0..n {
        let next = (i + 1) % n;
        buf.push_str(&format!("    s{} -> s{}\n", i, next));
    }
    // 一些跨边
    if n >= 6 {
        buf.push_str(&format!("    s{} -> s{}\n", 0, n / 2));
        buf.push_str(&format!("    s{} -> s{}\n", n / 2, 0));
    }
    buf.push_str("}\n");
}

fn gen_er_schema(buf: &mut String, n: usize) {
    buf.push_str(&format!("diagram er \"er-schema-{}\" {{\n\n", n));
    for i in 0..n {
        buf.push_str(&format!("    entity t{} \"Table{}\" {{\n        type: database\n        meta.pk: \"id\"\n        meta.fields: \"f1\\nf2\"\n    }}\n", i, i));
    }
    buf.push_str("\n");
    // 每个表连 1-2 个其他表
    for i in 0..n {
        let j = (i + 1) % n;
        buf.push_str(&format!("    t{} -> t{} \"fk\"\n", i, j));
        if n > 4 {
            let k = (i + 2) % n;
            if k != i && k != j {
                buf.push_str(&format!("    t{} -> t{} \"fk\"\n", i, k));
            }
        }
    }
    buf.push_str("}\n");
}

fn gen_ring(buf: &mut String, n: usize) {
    buf.push_str(&format!("diagram flowchart \"ring-{}\" {{\n\n", n));
    for i in 0..n {
        buf.push_str(&format!("    entity n{} \"N{}\" {{ type: process }}\n", i, i));
    }
    buf.push_str("\n");
    for i in 0..n {
        let next = (i + 1) % n;
        buf.push_str(&format!("    n{} -> n{}\n", i, next));
    }
    buf.push_str("}\n");
}

fn gen_path_shortcuts(buf: &mut String, n: usize) {
    buf.push_str(&format!("diagram flowchart \"pathsc-{}\" {{\n\n", n));
    for i in 0..n {
        let t = if i == 0 { "start" } else if i == n - 1 { "end" } else { "process" };
        buf.push_str(&format!("    entity n{} \"N{}\" {{ type: {} }}\n", i, i, t));
    }
    buf.push_str("\n");
    // 线性路径
    for i in 0..n.saturating_sub(1) {
        buf.push_str(&format!("    n{} -> n{}\n", i, i + 1));
    }
    // 捷径：每隔 3 个连一个跨边
    let mut i = 0;
    while i + 3 < n {
        buf.push_str(&format!("    n{} -> n{}\n", i, i + 3));
        i += 2;
    }
    buf.push_str("}\n");
}

fn gen_dual_hub(buf: &mut String, n: usize) {
    buf.push_str(&format!("diagram flowchart \"dualhub-{}\" {{\n\n", n));
    buf.push_str("    entity h1 \"Hub1\" { type: process }\n");
    buf.push_str("    entity h2 \"Hub2\" { type: process }\n");
    let half = n / 2;
    for i in 0..half {
        buf.push_str(&format!("    entity a{} \"A{}\" {{ type: process }}\n", i, i));
    }
    for i in 0..(n - half) {
        buf.push_str(&format!("    entity b{} \"B{}\" {{ type: process }}\n", i, i));
    }
    buf.push_str("\n");
    for i in 0..half {
        buf.push_str(&format!("    h1 -> a{}\n", i));
    }
    for i in 0..(n - half) {
        buf.push_str(&format!("    h2 -> b{}\n", i));
    }
    buf.push_str("    h1 -> h2\n");
    buf.push_str("}\n");
}

fn gen_sparse_layer(buf: &mut String, layers: usize, per_layer: usize) {
    buf.push_str(&format!("diagram flowchart \"sparse-L{}P{}\" {{\n\n", layers, per_layer));
    let mut id = 0usize;
    let mut layer_nodes: Vec<Vec<usize>> = Vec::new();
    for l in 0..layers {
        let mut layer = Vec::new();
        for _ in 0..per_layer {
            buf.push_str(&format!("    entity n{} \"N{}\" {{ type: process }}\n", id, id));
            layer.push(id);
            id += 1;
        }
        layer_nodes.push(layer);
    }
    buf.push_str("\n");
    // 层间稀疏：每层第一个节点连下一层第一个
    for l in 0..layers.saturating_sub(1) {
        buf.push_str(&format!("    n{} -> n{}\n", layer_nodes[l][0], layer_nodes[l + 1][0]));
        if per_layer > 1 {
            buf.push_str(&format!("    n{} -> n{}\n", layer_nodes[l][per_layer - 1], layer_nodes[l + 1][per_layer - 1]));
        }
    }
    buf.push_str("}\n");
}
