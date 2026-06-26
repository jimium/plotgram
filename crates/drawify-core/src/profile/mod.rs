//! 图表类型契约：每种 [`DiagramType`] 的默认布局、主题、实体类型等。
//!
//! [`profile_for`] 为查表入口。`type` 为各图规范枚举，不做跨图别名归一化。
//!
//! standard 属性键与值域契约见 [`standard_attrs`]，由 validation / expand 共享。

mod standard_attrs;

use std::sync::OnceLock;

use crate::types::{DiagramType, GraphicStyleId};
use crate::types::attr_constants::entity_type;

pub use standard_attrs::{STANDARD_ENTITY_ATTRS, STANDARD_RELATION_ATTRS};

#[derive(Debug)]
pub struct DiagramProfile {
    pub kind: DiagramType,
    pub name: &'static str,
    pub default_layout: &'static str,
    pub default_edge_routing: &'static str,
    pub default_theme_id: &'static str,
    pub dark_theme_id: Option<&'static str>,
    pub default_graphic_style: GraphicStyleId,
    pub entity_types: &'static [&'static str],
    /// 当 entity 缺少 `type` 属性时，由 `apply_profile_defaults` 补入的默认值。
    ///
    /// | DiagramType  | default_entity_type | 说明 |
    /// |--------------|---------------------|------|
    /// | State        | `"state"`           | 绝大多数节点是普通状态 |
    /// | Flowchart    | `"process"`         | 普通流程节点为默认 |
    /// | Sequence     | `"participant"`     | 生命线参与者为默认 |
    /// | Architecture | `"service"`         | 组件图默认形态 |
    /// | Mindmap      | `"branch"`          | 非 root 节点默认 |
    /// | Er           | `None`              | ER 实体无语义 type |
    pub default_entity_type: Option<&'static str>,
    /// 当 DSL 未声明 layout option 时，按图表类型补入的默认值（不覆盖用户显式配置）。
    pub default_layout_options: &'static [(&'static str, f64)],
    /// 用户未在 DSL 中声明 `direction` 时的默认值。
    ///
    /// `None` 表示该图类型不参与 direction 体系（布局默认算法不消费 direction）。
    /// 绑定 diagram type 而非 layout 算法——与 `default_layout` / `default_edge_routing` 对称。
    pub default_direction: Option<&'static str>,
    pub implemented: bool,
}

impl DiagramProfile {
    /// Entity 允许的 `attributes.standard` 键（与 validation 一致）。
    pub fn standard_entity_attrs(&self) -> &'static [&'static str] {
        STANDARD_ENTITY_ATTRS
    }

    /// Relation 允许的 `attributes.standard` 键（与 validation 一致）。
    pub fn standard_relation_attrs(&self) -> &'static [&'static str] {
        STANDARD_RELATION_ATTRS
    }

    /// 是否对 `type` 属性值做闭集校验（`entity_types` 非空时）。
    ///
    /// 为 `false` 时（如 ER）允许任意 atom 写法（如 `database`），与 expand 不补默认 type 的策略一致。
    pub fn restricts_entity_type_values(&self) -> bool {
        !self.entity_types.is_empty()
    }

    /// `default_entity_type`（若有）是否落在合法 type 值域内。
    pub fn default_entity_type_is_valid(&self) -> bool {
        self.default_entity_type
            .is_none_or(|entity_type| self.supports_entity_type(entity_type))
    }

    pub fn supports_entity_type(&self, entity_type: &str) -> bool {
        if self.entity_types.is_empty() {
            return true;
        }
        self.entity_types.contains(&entity_type)
    }
}

const DEFAULT_DARK_THEME_ID: &str = "common.clean-dark";
const DEFAULT_GRAPHIC_STYLE: GraphicStyleId = GraphicStyleId::Standard;

const FLOWCHART_ENTITY_TYPES: &[&str] = &[
    entity_type::SERVICE,
    entity_type::DATABASE,
    entity_type::PERSON,
    entity_type::CLIENT,
    entity_type::QUEUE,
    entity_type::CACHE,
    entity_type::GATEWAY,
    entity_type::STORAGE,
    entity_type::EXTERNAL,
    entity_type::DECISION,
    entity_type::PROCESS,
    entity_type::START,
    entity_type::END,
];

const SEQUENCE_ENTITY_TYPES: &[&str] = &[
    entity_type::PARTICIPANT,
    entity_type::ACTOR,
    entity_type::BOUNDARY,
    entity_type::CONTROL,
    entity_type::LIFELINE,
    entity_type::DATABASE,
];

/// 架构图规范实体类型
const ARCHITECTURE_ENTITY_TYPES: &[&str] = &[
    entity_type::FRONTEND,
    entity_type::BACKEND,
    entity_type::SERVICE,
    entity_type::DATABASE,
    entity_type::GATEWAY,
    entity_type::CACHE,
    entity_type::QUEUE,
    entity_type::STORAGE,
    entity_type::EXTERNAL,
];

const STATE_ENTITY_TYPES: &[&str] = &[
    entity_type::INITIAL,
    entity_type::STATE,
    entity_type::FINAL,
    entity_type::CHOICE,
];
const MINDMAP_ENTITY_TYPES: &[&str] = &[
    entity_type::ROOT,
    entity_type::MAIN,
    entity_type::BRANCH,
    entity_type::LEAF,
];

static FLOWCHART_PROFILE: DiagramProfile = DiagramProfile {
    kind: DiagramType::Flowchart,
    name: "flowchart",
    default_layout: "flowchart",
    default_edge_routing: "orthogonal",
    default_theme_id: "common.clean-light",
    dark_theme_id: Some(DEFAULT_DARK_THEME_ID),
    default_graphic_style: DEFAULT_GRAPHIC_STYLE,
    entity_types: FLOWCHART_ENTITY_TYPES,
    default_entity_type: Some(entity_type::PROCESS),
    default_layout_options: &[],
    default_direction: Some(crate::types::attr_constants::direction::TOP_TO_BOTTOM),
    implemented: true,
};

static SEQUENCE_PROFILE: DiagramProfile = DiagramProfile {
    kind: DiagramType::Sequence,
    name: "sequence",
    default_layout: "sequence",
    default_edge_routing: "",
    default_theme_id: "common.clean-light",
    dark_theme_id: Some(DEFAULT_DARK_THEME_ID),
    default_graphic_style: DEFAULT_GRAPHIC_STYLE,
    entity_types: SEQUENCE_ENTITY_TYPES,
    default_entity_type: Some(entity_type::PARTICIPANT),
    default_layout_options: &[],
    default_direction: None,
    implemented: true,
};

static ARCHITECTURE_PROFILE: DiagramProfile = DiagramProfile {
    kind: DiagramType::Architecture,
    name: "architecture",
    default_layout: "architecture",
    default_edge_routing: "orthogonal",
    default_theme_id: "common.blueprint",
    dark_theme_id: Some(DEFAULT_DARK_THEME_ID),
    default_graphic_style: DEFAULT_GRAPHIC_STYLE,
    entity_types: ARCHITECTURE_ENTITY_TYPES,
    default_entity_type: Some(entity_type::SERVICE),
    default_layout_options: &[],
    default_direction: Some(crate::types::attr_constants::direction::TOP_TO_BOTTOM),
    implemented: true,
};

static STATE_PROFILE: DiagramProfile = DiagramProfile {
    kind: DiagramType::State,
    name: "state",
    default_layout: "state",
    default_edge_routing: "circular",
    default_theme_id: "common.clean-light",
    dark_theme_id: Some(DEFAULT_DARK_THEME_ID),
    default_graphic_style: DEFAULT_GRAPHIC_STYLE,
    entity_types: STATE_ENTITY_TYPES,
    default_entity_type: Some(entity_type::STATE),
    default_layout_options: &[],
    default_direction: None,
    implemented: true,
};

static ER_PROFILE: DiagramProfile = DiagramProfile {
    kind: DiagramType::Er,
    name: "er",
    default_layout: "er",
    default_edge_routing: "straight",
    default_theme_id: "common.blueprint",
    dark_theme_id: Some(DEFAULT_DARK_THEME_ID),
    default_graphic_style: DEFAULT_GRAPHIC_STYLE,
    entity_types: &[],
    default_entity_type: None,
    default_layout_options: &[],
    default_direction: Some(crate::types::attr_constants::direction::TOP_TO_BOTTOM),
    implemented: true,
};

static MINDMAP_PROFILE: DiagramProfile = DiagramProfile {
    kind: DiagramType::Mindmap,
    name: "mindmap",
    default_layout: "mindmap",
    default_edge_routing: "organic",
    default_theme_id: "mindmap.vivid-branches",
    dark_theme_id: Some("mindmap.ink-dark"),
    default_graphic_style: DEFAULT_GRAPHIC_STYLE,
    entity_types: MINDMAP_ENTITY_TYPES,
    default_entity_type: Some(entity_type::BRANCH),
    default_layout_options: &[],
    default_direction: Some(crate::types::attr_constants::direction::RADIAL),
    implemented: true,
};

pub fn profile_for(diagram_type: &DiagramType) -> &'static DiagramProfile {
    match diagram_type {
        DiagramType::Flowchart => &FLOWCHART_PROFILE,
        DiagramType::Sequence => &SEQUENCE_PROFILE,
        DiagramType::Architecture => &ARCHITECTURE_PROFILE,
        DiagramType::State => &STATE_PROFILE,
        DiagramType::Er => &ER_PROFILE,
        DiagramType::Mindmap => &MINDMAP_PROFILE,
        DiagramType::Custom(name) => custom_profile(name),
    }
}

pub fn builtin_profiles() -> [&'static DiagramProfile; 6] {
    [
        &FLOWCHART_PROFILE,
        &SEQUENCE_PROFILE,
        &ARCHITECTURE_PROFILE,
        &STATE_PROFILE,
        &ER_PROFILE,
        &MINDMAP_PROFILE,
    ]
}

fn custom_profile(name: &str) -> &'static DiagramProfile {
    static CUSTOM_PROFILE: OnceLock<DiagramProfile> = OnceLock::new();

    CUSTOM_PROFILE.get_or_init(|| DiagramProfile {
        kind: DiagramType::Custom(name.to_string()),
        name: "custom",
        default_layout: FLOWCHART_PROFILE.default_layout,
        default_edge_routing: FLOWCHART_PROFILE.default_edge_routing,
        default_theme_id: FLOWCHART_PROFILE.default_theme_id,
        dark_theme_id: FLOWCHART_PROFILE.dark_theme_id,
        default_graphic_style: FLOWCHART_PROFILE.default_graphic_style,
        entity_types: FLOWCHART_PROFILE.entity_types,
        default_entity_type: FLOWCHART_PROFILE.default_entity_type,
        default_layout_options: FLOWCHART_PROFILE.default_layout_options,
        default_direction: FLOWCHART_PROFILE.default_direction,
        implemented: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_profiles_have_consistent_default_entity_type() {
        for profile in builtin_profiles() {
            assert!(
                profile.default_entity_type_is_valid(),
                "profile {:?} has invalid default_entity_type {:?}",
                profile.kind,
                profile.default_entity_type
            );
        }
    }

    #[test]
    fn er_profile_allows_open_entity_type_values() {
        let profile = profile_for(&DiagramType::Er);
        assert!(!profile.restricts_entity_type_values());
        assert!(profile.supports_entity_type("database"));
        assert!(profile.default_entity_type.is_none());
    }

    #[test]
    fn state_profile_restricts_entity_type_values() {
        let profile = profile_for(&DiagramType::State);
        assert!(profile.restricts_entity_type_values());
        assert!(profile.supports_entity_type("state"));
        assert!(!profile.supports_entity_type("actor"));
    }

    #[test]
    fn all_builtin_diagram_types_have_profiles() {
        for kind in [
            DiagramType::Flowchart,
            DiagramType::Sequence,
            DiagramType::Architecture,
            DiagramType::State,
            DiagramType::Er,
            DiagramType::Mindmap,
        ] {
            let profile = profile_for(&kind);
            assert!(!profile.default_layout.is_empty());
            assert!(!profile.default_theme_id.is_empty());
            assert_eq!(profile.default_graphic_style.as_str(), "standard");

            if kind != DiagramType::Sequence {
                assert!(!profile.default_edge_routing.is_empty());
            }
        }
    }

    #[test]
    fn custom_diagram_type_uses_compat_profile() {
        let profile = profile_for(&DiagramType::Custom("custom-x".to_string()));
        assert_eq!(profile.name, "custom");
        assert_eq!(profile.default_layout, "flowchart");
        assert_eq!(profile.default_edge_routing, "orthogonal");
    }

    #[test]
    fn architecture_profile_is_implemented() {
        let profile = profile_for(&DiagramType::Architecture);
        assert!(profile.implemented);
        assert_eq!(profile.default_layout, "architecture");
    }

    #[test]
    fn sequence_profile_has_no_edge_routing() {
        let profile = profile_for(&DiagramType::Sequence);
        assert_eq!(profile.default_layout, "sequence");
        assert_eq!(profile.default_edge_routing, "");
    }

    #[test]
    fn er_profile_is_implemented() {
        let profile = profile_for(&DiagramType::Er);
        assert!(profile.implemented);
        assert_eq!(profile.default_layout, "er");
        assert_eq!(profile.default_edge_routing, "straight");
    }

    #[test]
    fn state_profile_uses_state_layout() {
        let profile = profile_for(&DiagramType::State);
        assert!(profile.implemented);
        assert_eq!(profile.default_layout, "state");
        assert_eq!(profile.default_edge_routing, "circular");
    }

    #[test]
    fn mindmap_profile_is_implemented() {
        let profile = profile_for(&DiagramType::Mindmap);
        assert!(profile.implemented);
        assert_eq!(profile.default_layout, "mindmap");
        assert_eq!(profile.default_edge_routing, "organic");
    }

    #[test]
    fn default_direction_matches_spec() {
        use crate::types::attr_constants::direction;

        let flowchart = profile_for(&DiagramType::Flowchart);
        assert_eq!(flowchart.default_direction, Some(direction::TOP_TO_BOTTOM));

        let architecture = profile_for(&DiagramType::Architecture);
        assert_eq!(architecture.default_direction, Some(direction::TOP_TO_BOTTOM));

        let er = profile_for(&DiagramType::Er);
        assert_eq!(er.default_direction, Some(direction::TOP_TO_BOTTOM));

        let mindmap = profile_for(&DiagramType::Mindmap);
        assert_eq!(mindmap.default_direction, Some(direction::RADIAL));

        let state = profile_for(&DiagramType::State);
        assert_eq!(state.default_direction, None);

        let sequence = profile_for(&DiagramType::Sequence);
        assert_eq!(sequence.default_direction, None);
    }

    #[test]
    fn custom_profile_inherits_flowchart_default_direction() {
        use crate::types::attr_constants::direction;
        let profile = profile_for(&DiagramType::Custom("test".to_string()));
        assert_eq!(profile.default_direction, Some(direction::TOP_TO_BOTTOM));
    }
}
