//! 评估结果持久化与历史追踪模块
//!
//! 支持将评估结果保存为 JSON 文件，并在后续运行中对比历史结果。
//! 用于：
//! - 算法改进前后的自动对比
//! - 回归检测
//! - 性能趋势追踪

use crate::report::EvalReport;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// 历史结果存储
pub struct HistoryStore {
    /// 存储目录
    dir: PathBuf,
}

/// 历史记录条目
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    /// 标签（如 "baseline", "after-optimization"）
    pub label: String,
    /// 文件路径
    pub path: PathBuf,
    /// 修改时间
    pub modified: SystemTime,
}

impl HistoryStore {
    /// 创建历史存储（自动创建目录）
    pub fn new(dir: &Path) -> std::io::Result<Self> {
        fs::create_dir_all(dir)?;
        Ok(Self {
            dir: dir.to_path_buf(),
        })
    }

    /// 保存评估报告
    pub fn save(&self, report: &EvalReport, label: &str) -> std::io::Result<PathBuf> {
        let timestamp = chrono_independent_timestamp();
        let filename = format!("{}_{}.json", label, timestamp);
        let path = self.dir.join(&filename);
        let json = report.to_json();
        fs::write(&path, json)?;
        Ok(path)
    }

    /// 加载指定标签的最新报告
    pub fn load_latest(&self) -> std::io::Result<EvalReport> {
        let entries = self.list()?;
        let latest = entries
            .into_iter()
            .max_by_key(|e| e.modified)
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "No history entries"))?;
        self.load_from_path(&latest.path)
    }

    /// 加载指定标签的报告
    pub fn load(&self, label: &str) -> std::io::Result<EvalReport> {
        let entries = self.list()?;
        let entry = entries
            .into_iter()
            .filter(|e| e.label == label)
            .max_by_key(|e| e.modified)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    format!("No entry with label '{}'", label),
                )
            })?;
        self.load_from_path(&entry.path)
    }

    /// 列出所有历史记录
    pub fn list(&self) -> std::io::Result<Vec<HistoryEntry>> {
        let mut entries = Vec::new();
        if !self.dir.exists() {
            return Ok(entries);
        }

        for entry in fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                let metadata = entry.metadata()?;
                let modified = metadata.modified()?;

                let label = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                entries.push(HistoryEntry {
                    label,
                    path,
                    modified,
                });
            }
        }

        entries.sort_by(|a, b| b.modified.cmp(&a.modified));
        Ok(entries)
    }

    /// 与最新历史结果对比，返回差异报告
    pub fn compare_with_latest(
        &self,
        current: &EvalReport,
        engine: &crate::engine::EvalEngine,
    ) -> Option<Vec<crate::engine::DiffReport>> {
        let latest = self.load_latest().ok()?;
        let mut diffs = Vec::new();

        for curr_comp in &current.comparisons {
            for hist_comp in &latest.comparisons {
                if curr_comp.diagram_name != hist_comp.diagram_name {
                    continue;
                }

                // 找到相同算法的结果进行对比
                for curr_result in &curr_comp.results {
                    for hist_result in &hist_comp.results {
                        if curr_result.algorithm == hist_result.algorithm {
                            let mut diff = engine.diff(hist_result, curr_result);
                            diff.diagram_name = curr_comp.diagram_name.clone();
                            diffs.push(diff);
                        }
                    }
                }
            }
        }

        if diffs.is_empty() {
            None
        } else {
            Some(diffs)
        }
    }

    fn load_from_path(&self, path: &Path) -> std::io::Result<EvalReport> {
        let json = fs::read_to_string(path)?;
        serde_json::from_str(&json).map_err(|e| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to parse history file: {}", e),
            )
        })
    }
}

/// 生成不依赖 chrono 的时间戳字符串
fn chrono_independent_timestamp() -> String {
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{presets, EvalEngine};
    use drawify_core::ast::*;
    use drawify_core::types::DiagramType;
    use std::path::PathBuf;

    fn sample_diagram() -> Diagram {
        Diagram {
            diagram_type: DiagramType::Flowchart,
            attributes: vec![],
            entities: vec![
                Entity {
                    id: Identifier::new_unchecked("a"),
                    label: "A".to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span: Span::dummy(),
                },
                Entity {
                    id: Identifier::new_unchecked("b"),
                    label: "B".to_string(),
                    attributes: AttributeMap::default(),
                    group_id: None,
                    span: Span::dummy(),
                },
            ],
            relations: vec![Relation {
                from: Identifier::new_unchecked("a"),
                to: Identifier::new_unchecked("b"),
                arrow: ArrowType::Active,
                label: None,
                head_label: None,
                tail_label: None,
                attributes: AttributeMap::default(),
                span: Span::dummy(),
            }],
            groups: vec![],
            style_decls: vec![],
            doc_comment: None,
            source_info: SourceInfo {
                file: None,
                line_count: 1,
            },
        }
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("drawify-eval-test-{}", name));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn test_history_save_and_load() {
        let dir = temp_dir("save-load");
        let store = HistoryStore::new(&dir).unwrap();

        let engine = EvalEngine::new();
        let diagram = sample_diagram();
        let configs = presets::routing_comparison();
        let comp = engine.compare("test", &diagram, &configs);

        let mut report = EvalReport::new("Test");
        report.add_comparison(comp);

        let path = store.save(&report, "baseline").unwrap();
        assert!(path.exists());

        let loaded = store.load_latest().unwrap();
        assert_eq!(loaded.title, "Test");

        // 清理
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_history_list() {
        let dir = temp_dir("list");
        let store = HistoryStore::new(&dir).unwrap();

        let engine = EvalEngine::new();
        let diagram = sample_diagram();
        let configs = presets::routing_comparison();
        let comp = engine.compare("test", &diagram, &configs);

        let mut report = EvalReport::new("Test");
        report.add_comparison(comp);

        store.save(&report, "run1").unwrap();
        store.save(&report, "run2").unwrap();

        let entries = store.list().unwrap();
        assert!(entries.len() >= 2, "Expected at least 2 entries, got {}", entries.len());

        // 清理
        let _ = fs::remove_dir_all(&dir);
    }
}
