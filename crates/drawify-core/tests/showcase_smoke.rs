//! Showcase 冒烟测试：parse → prepare → validate → render(svg)

use drawify_core::layout::compute_layout;
use drawify_core::prepare::StyleRequest;
use drawify_core::pipeline::{parse, prepare, render_text};
use drawify_core::render::{RenderFormat, RenderRequest};
use drawify_core::validation::validate;
use std::fs;
use std::path::{Path, PathBuf};

fn showcase_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../showcase")
}

fn collect_dfy_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).expect("read showcase dir") {
        let entry = entry.expect("read entry");
        let path = entry.path();
        if path.is_dir() {
            collect_dfy_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "dfy") {
            out.push(path);
        }
    }
}

#[test]
fn showcase_parse_prepare_validate_render() {
    let root = showcase_root();
    let mut files = Vec::new();
    collect_dfy_files(&root, &mut files);
    files.sort();
    assert!(!files.is_empty(), "expected at least one showcase .dfy file");

    let mut failures = Vec::new();

    for path in &files {
        let rel = path.strip_prefix(&root).unwrap().display();
        let source = fs::read_to_string(path).expect("read dfy");

        let raw = match parse(&source) {
            Ok(r) => r,
            Err(e) => {
                failures.push(format!("PARSE {rel}: {e}"));
                continue;
            }
        };

        let output = match prepare(raw, &StyleRequest::default()) {
            Ok(o) => o,
            Err(e) => {
                failures.push(format!("PREPARE {rel}: {e}"));
                continue;
            }
        };
        let prepared = &output.diagram;

        let val = validate(prepared);
        if val.has_errors() {
            failures.push(format!(
                "VALIDATE {rel}: {} errors (first: {})",
                val.errors.len(),
                val.errors
                    .first()
                    .map(|e| e.to_string())
                    .unwrap_or_default()
            ));
            continue;
        }

        match render_text(&RenderRequest::new(&prepared, RenderFormat::Svg)) {
            Ok(svg) if svg.contains("<svg") => {}
            Ok(_) => failures.push(format!("RENDER {rel}: output missing <svg")),
            Err(e) => failures.push(format!("RENDER {rel}: {e}")),
        }
    }

    for f in &failures {
        eprintln!("{f}");
    }

    assert!(
        failures.is_empty(),
        "{} showcase file(s) failed",
        failures.len()
    );
}

/// 布局质量回归测试：所有架构图 showcase 的节点必须在所属分组边界内。
///
/// 防止 V2 adjuster / refine / grid snap 等后处理步骤将节点推到分组外。
#[test]
fn showcase_architecture_group_containment() {
    let root = showcase_root();
    let arch_dir = root.join("architecture");
    let mut files = Vec::new();
    collect_dfy_files(&arch_dir, &mut files);
    files.sort();
    assert!(
        !files.is_empty(),
        "expected at least one architecture showcase .dfy file"
    );

    let mut failures: Vec<String> = Vec::new();

    for path in &files {
        let rel = path.strip_prefix(&root).unwrap().display();
        let source = fs::read_to_string(path).expect("read dfy");

        let raw = match parse(&source) {
            Ok(r) => r,
            Err(_) => continue, // 解析失败由冒烟测试覆盖
        };

        let output = match prepare(raw, &StyleRequest::default()) {
            Ok(o) => o,
            Err(_) => continue,
        };
        let prepared = &output.diagram;

        // 只检查有分组的图
        if prepared.groups.is_empty() {
            continue;
        }

        let layout = match compute_layout(prepared.inner()) {
            Ok(l) => l,
            Err(e) => {
                failures.push(format!("LAYOUT {rel}: {e}"));
                continue;
            }
        };

        let violations = layout.validate_group_containment(prepared.inner());
        for v in &violations {
            let dir = match v.kind {
                drawify_core::layout::ContainmentViolationKind::TopOverflow => "top",
                drawify_core::layout::ContainmentViolationKind::BottomOverflow => "bottom",
                drawify_core::layout::ContainmentViolationKind::LeftOverflow => "left",
                drawify_core::layout::ContainmentViolationKind::RightOverflow => "right",
            };
            failures.push(format!(
                "CONTAINMENT {rel}: '{}' {} overflow in group '{}' by {:.1}px",
                v.entity_id, dir, v.group_id, v.excess
            ));
        }
    }

    for f in &failures {
        eprintln!("{f}");
    }

    assert!(
        failures.is_empty(),
        "{} group containment violation(s) detected",
        failures.len()
    );
}
