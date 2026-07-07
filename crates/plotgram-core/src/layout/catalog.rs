//! 布局/边路由能力目录（供 Playground、CLI、文档生成等查询）。
//!
//! 数据来源于 `profile`（默认算法）与算法注册表（适用关系），
//! 保证与 `compute_layout` 校验逻辑一致。

use serde::Serialize;

use crate::types::DiagramType;
use crate::profile::profile_for;

use super::algorithm_config::{AlgorithmOptionSpec, OptionKind};
use super::{applicable_layouts_for_type, applicable_routings_for_type, BUILTIN_DIAGRAM_TYPES};

/// 单个算法 option 的可序列化描述（供 catalog / WASM 导出）。
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct AlgorithmOptionInfo {
    pub key: String,
    pub kind: String,
    pub default: f64,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exclude_min: Option<bool>,
}

impl From<&AlgorithmOptionSpec> for AlgorithmOptionInfo {
    fn from(spec: &AlgorithmOptionSpec) -> Self {
        let (kind, min, max, exclude_min) = match spec.kind {
            OptionKind::NonNegativeNumber => ("non_negative_number".to_string(), Some(0.0), None, None),
            OptionKind::PositiveNumber => ("positive_number".to_string(), None, None, None),
            OptionKind::Number {
                min,
                max,
                exclude_min,
            } => (
                "number".to_string(),
                Some(min),
                Some(max),
                exclude_min.then_some(true),
            ),
        };
        Self {
            key: spec.key.to_string(),
            kind,
            default: spec.default,
            description: spec.description.to_string(),
            min,
            max,
            exclude_min,
        }
    }
}

fn option_infos(specs: &'static [AlgorithmOptionSpec]) -> Vec<AlgorithmOptionInfo> {
    specs.iter().map(AlgorithmOptionInfo::from).collect()
}

/// 单个布局算法的元信息。
#[derive(Debug, Clone, Serialize)]
pub struct LayoutAlgoInfo {
    pub name: String,
    /// 为 `true` 时布局阶段已产出边几何，`edge_routing` 不会生效。
    pub produces_edge_geometry: bool,
    pub options: Vec<AlgorithmOptionInfo>,
    /// 该算法支持的方向列表。空列表表示不消费 diagram 级 `direction`。
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub supported_directions: Vec<String>,
}

/// 单个边路由算法的元信息。
#[derive(Debug, Clone, Serialize)]
pub struct EdgeRoutingAlgoInfo {
    pub name: String,
    pub options: Vec<AlgorithmOptionInfo>,
}

/// 某种图表类型的布局能力。
#[derive(Debug, Clone, Serialize)]
pub struct DiagramTypeCatalog {
    pub name: String,
    pub implemented: bool,
    pub default_layout: String,
    /// 空字符串表示该图类型不使用边路由（如时序图）。
    pub default_edge_routing: String,
    /// 为 `false` 时 DSL 不得声明 `edge_routing`（边几何由布局算法内置）。
    pub uses_edge_routing: bool,
    pub layouts: Vec<String>,
    pub edge_routings: Vec<String>,
    /// 用户未在 DSL 中声明 `direction` 时的默认值。
    /// `None` 表示该图类型不参与 direction 体系。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_direction: Option<String>,
}

/// 全局布局/边路由能力目录。
#[derive(Debug, Clone, Serialize)]
pub struct LayoutCatalog {
    /// 所有已注册布局算法（含元信息）。
    pub layouts: Vec<LayoutAlgoInfo>,
    /// 所有已注册边路由算法（含 option 元信息）。
    pub edge_routings: Vec<EdgeRoutingAlgoInfo>,
    /// 各内置图表类型的默认与可选算法。
    pub diagram_types: Vec<DiagramTypeCatalog>,
}

/// 构建完整布局能力目录。
pub fn layout_catalog() -> LayoutCatalog {
    LayoutCatalog {
        layouts: all_layout_algo_info(),
        edge_routings: all_edge_routing_algo_info(),
        diagram_types: builtin_diagram_type_catalogs(),
    }
}

fn all_layout_algo_info() -> Vec<LayoutAlgoInfo> {
    super::all_layout_strategies()
        .into_iter()
        .map(|s| LayoutAlgoInfo {
            name: s.name().to_string(),
            produces_edge_geometry: s.produces_edge_geometry(),
            options: option_infos(s.option_specs()),
            supported_directions: s.supported_directions().iter().map(|d| d.to_string()).collect(),
        })
        .collect()
}

fn all_edge_routing_algo_info() -> Vec<EdgeRoutingAlgoInfo> {
    super::all_routing_strategies()
        .into_iter()
        .map(|s| EdgeRoutingAlgoInfo {
            name: s.name().to_string(),
            options: option_infos(s.option_specs()),
        })
        .collect()
}

fn builtin_diagram_type_catalogs() -> Vec<DiagramTypeCatalog> {
    BUILTIN_DIAGRAM_TYPES
        .iter()
        .map(|kind| diagram_type_catalog(kind))
        .collect()
}

fn diagram_type_catalog(kind: &DiagramType) -> DiagramTypeCatalog {
    let profile = profile_for(kind);
    let default_layout = profile.default_layout.to_string();
    let layouts = sort_with_default_first(
        applicable_layouts_for_type(kind)
            .into_iter()
            .map(|s| s.to_string())
            .collect(),
        &default_layout,
    );
    let default_edge_routing = profile.default_edge_routing.to_string();
    let edge_routings = sort_with_default_first(
        applicable_routings_for_type(kind)
            .into_iter()
            .map(|s| s.to_string())
            .collect(),
        &default_edge_routing,
    );

    let uses_edge_routing = !edge_routings.is_empty();
    let default_direction = profile.default_direction.map(|s| s.to_string());

    DiagramTypeCatalog {
        name: profile.name.to_string(),
        implemented: profile.implemented,
        default_layout,
        default_edge_routing: if uses_edge_routing {
            default_edge_routing
        } else {
            String::new()
        },
        uses_edge_routing,
        layouts,
        edge_routings,
        default_direction,
    }
}

fn sort_with_default_first(mut items: Vec<String>, default: &str) -> Vec<String> {
    items.sort();
    if let Some(pos) = items.iter().position(|s| s == default) {
        let d = items.remove(pos);
        items.insert(0, d);
    }
    items
}

/// `custom` 图表类型能力（继承 flowchart profile）。
pub fn custom_diagram_type_catalog() -> DiagramTypeCatalog {
    diagram_type_catalog(&DiagramType::Custom(String::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_matches_registry_defaults() {
        let catalog = layout_catalog();
        let flowchart = catalog
            .diagram_types
            .iter()
            .find(|d| d.name == "flowchart")
            .expect("flowchart entry");
        assert_eq!(flowchart.default_layout, "flowchart");
        assert_eq!(flowchart.default_edge_routing, "orthogonal");
        assert!(flowchart.layouts.contains(&"flowchart".to_string()));
        assert!(flowchart.layouts.contains(&"sugiyama-v2".to_string()));
    }

    #[test]
    fn sequence_layout_produces_edges() {
        let catalog = layout_catalog();
        let sequence = catalog
            .layouts
            .iter()
            .find(|l| l.name == "sequence")
            .expect("sequence layout");
        assert!(sequence.produces_edge_geometry);
    }

    #[test]
    fn sequence_does_not_use_edge_routing() {
        let catalog = layout_catalog();
        let sequence = catalog
            .diagram_types
            .iter()
            .find(|d| d.name == "sequence")
            .expect("sequence entry");
        assert!(!sequence.uses_edge_routing);
        assert!(sequence.edge_routings.is_empty());
        assert!(sequence.default_edge_routing.is_empty());
    }

    #[test]
    fn architecture_defaults_to_architecture() {
        let catalog = layout_catalog();
        let arch = catalog
            .diagram_types
            .iter()
            .find(|d| d.name == "architecture")
            .expect("architecture entry");
        assert_eq!(arch.default_layout, "architecture");
    }

    #[test]
    fn mindmap_is_implemented() {
        let catalog = layout_catalog();
        let mindmap = catalog
            .diagram_types
            .iter()
            .find(|d| d.name == "mindmap")
            .expect("mindmap entry");
        assert!(mindmap.implemented);
        assert_eq!(mindmap.default_layout, "mindmap");
        assert_eq!(mindmap.default_edge_routing, "organic");
    }

    #[test]
    fn er_defaults_to_er_layout() {
        let catalog = layout_catalog();
        let er = catalog
            .diagram_types
            .iter()
            .find(|d| d.name == "er")
            .expect("er entry");
        assert_eq!(er.default_layout, "er");
        assert!(er.layouts.contains(&"er".to_string()));
    }

    #[test]
    fn layout_algo_options_are_exported() {
        let catalog = layout_catalog();
        let sugiyama = catalog
            .layouts
            .iter()
            .find(|l| l.name == "sugiyama-v2")
            .expect("sugiyama-v2");
        assert_eq!(sugiyama.options.len(), 1);
        assert_eq!(sugiyama.options[0].key, "group_padding");
        assert_eq!(sugiyama.options[0].kind, "non_negative_number");
    }

    #[test]
    fn edge_routing_options_are_exported() {
        let catalog = layout_catalog();
        let bezier = catalog
            .edge_routings
            .iter()
            .find(|r| r.name == "bezier")
            .expect("bezier");
        assert_eq!(bezier.options.len(), 1);
        assert_eq!(bezier.options[0].key, "tension");

        let orthogonal = catalog
            .edge_routings
            .iter()
            .find(|r| r.name == "orthogonal")
            .expect("orthogonal");
        assert_eq!(orthogonal.options.len(), 3);
        let keys: Vec<&str> = orthogonal.options.iter().map(|o| o.key.as_str()).collect();
        assert!(keys.contains(&"slot_pitch"));
        assert!(keys.contains(&"channel_margin"));
        assert!(keys.contains(&"bundling"));
    }
}
