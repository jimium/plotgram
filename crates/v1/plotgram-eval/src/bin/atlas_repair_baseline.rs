//! M7-0：Ink 后 dogleg repair 基线探针。
//!
//! 对 product-regression 中 flowchart/architecture 图跑完整布局，统计
//! `hints.atlas_plan_distorted_edges`（第三道穿组硬修触发的失真边）。
//!
//! ```text
//! cargo run -p plotgram-eval --bin atlas_repair_baseline -- \
//!   benchmarks/sets/product-regression-set.txt
//! ```

use plotgram_core::layout::compute_layout_with_plan;
use plotgram_core::pipeline::parse_prepare_validate;
use plotgram_core::prepare::StyleRequest;
use serde::Serialize;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

#[derive(Debug, Clone, Serialize)]
struct Row {
    path: String,
    groups: usize,
    edges: usize,
    distorted: usize,
    distorted_eids: Vec<usize>,
    /// no_groups | clean | repaired/gates_ok | repaired/gates_thin | skip | error
    class: String,
    note: String,
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut json = false;
    let mut set_path = PathBuf::from("benchmarks/sets/product-regression-set.txt");
    for a in &args {
        if a == "--json" {
            json = true;
        } else if !a.starts_with('-') {
            set_path = PathBuf::from(a);
        }
    }

    let root = find_repo_root();
    let set_abs = if set_path.is_absolute() {
        set_path.clone()
    } else {
        root.join(&set_path)
    };

    let paths = read_set(&set_abs).unwrap_or_else(|e| {
        eprintln!("FAIL: read set {}: {e}", set_abs.display());
        process::exit(1);
    });

    let mut rows = Vec::new();
    for rel in &paths {
        let is_hier_candidate =
            rel.starts_with("showcase/flowchart/") || rel.starts_with("showcase/architecture/");
        if !is_hier_candidate {
            rows.push(Row {
                path: rel.clone(),
                groups: 0,
                edges: 0,
                distorted: 0,
                distorted_eids: vec![],
                class: "skip".into(),
                note: "non flowchart/architecture".into(),
            });
            continue;
        }

        let abs = root.join(rel);
        match measure_one(&abs) {
            Ok(mut row) => {
                row.path = rel.clone();
                rows.push(row);
            }
            Err(e) => {
                rows.push(Row {
                    path: rel.clone(),
                    groups: 0,
                    edges: 0,
                    distorted: 0,
                    distorted_eids: vec![],
                    class: "error".into(),
                    note: e,
                });
            }
        }
    }

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&rows).expect("serialize")
        );
    } else {
        println!("path\tgroups\tedges\tdistorted\tdistorted_eids\tclass\tnote");
        for r in &rows {
            let eids = r
                .distorted_eids
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(",");
            println!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}",
                r.path, r.groups, r.edges, r.distorted, eids, r.class, r.note
            );
        }
        let hier: Vec<_> = rows
            .iter()
            .filter(|r| r.class != "skip" && r.class != "error")
            .collect();
        let total_distorted: usize = hier.iter().map(|r| r.distorted).sum();
        let repaired_n = hier.iter().filter(|r| r.distorted > 0).count();
        eprintln!(
            "# summary: hier_measured={} repaired_diagrams={} total_distorted_edges={}",
            hier.len(),
            repaired_n,
            total_distorted
        );
    }
}

fn measure_one(path: &Path) -> Result<Row, String> {
    let source = fs::read_to_string(path).map_err(|e| format!("read: {e}"))?;
    let output = parse_prepare_validate(&source, &StyleRequest::default());
    if !output.is_valid() {
        return Err(format!("parse/validate: {:?}", output.errors));
    }
    let prepared = output.diagram.ok_or_else(|| "no diagram".to_string())?;
    let diagram = prepared.inner();
    let layout = compute_layout_with_plan(diagram, prepared.layout_plan())
        .map_err(|e| format!("layout: {e}"))?;

    let groups = layout.groups.len();
    let edges = layout.edges.len();
    let mut distorted_eids = layout.hints.atlas_plan_distorted_edges.clone();
    distorted_eids.sort_unstable();
    distorted_eids.dedup();
    let distorted = distorted_eids.len();

    let (class, note) = if groups == 0 {
        ("no_groups".into(), "no groups → repair N/A".into())
    } else if distorted == 0 {
        ("clean".into(), "groups present, no dogleg".into())
    } else {
        match layout.hints.atlas_plan.as_ref() {
            None => (
                "repaired/gates_thin".into(),
                "distorted but atlas_plan=None".into(),
            ),
            Some(plan) => {
                let thin = distorted_eids.iter().any(|&eid| {
                    plan.gates
                        .get(&eid)
                        .map(|g| g.is_empty())
                        .unwrap_or(true)
                });
                if thin {
                    (
                        "repaired/gates_thin".into(),
                        "some distorted edges missing/empty gates".into(),
                    )
                } else {
                    (
                        "repaired/gates_ok".into(),
                        "all distorted edges have non-empty gates".into(),
                    )
                }
            }
        }
    };

    Ok(Row {
        path: String::new(),
        groups,
        edges,
        distorted,
        distorted_eids,
        class,
        note,
    })
}

fn read_set(path: &Path) -> Result<Vec<String>, String> {
    let text = fs::read_to_string(path).map_err(|e| e.to_string())?;
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|s| s.to_string())
        .collect())
}

fn find_repo_root() -> PathBuf {
    let mut dir = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    for _ in 0..8 {
        if dir.join("Cargo.toml").is_file() && dir.join("showcase").is_dir() {
            return dir;
        }
        if !dir.pop() {
            break;
        }
    }
    env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}
