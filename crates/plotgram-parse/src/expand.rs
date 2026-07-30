//! Post-lower expands: `@group` sugar, structural attr lift, profile defaults.

use crate::error::ParseError;
use plotgram_model::attr::AttrMap;
use plotgram_model::contract::AlgorithmRef;
use plotgram_model::graph::Graph;
use plotgram_model::profile::{DiagramType, Profile};
use plotgram_model::render::RenderMeta;

/// Diagram-level fields collected during lower / expand.
#[derive(Debug, Clone, Default)]
pub struct DiagramMeta {
    pub profile: Option<DiagramType>,
    pub layout: Option<AlgorithmRef>,
    pub edge_routing: Option<AlgorithmRef>,
    pub title: Option<String>,
    pub theme: Option<String>,
    pub render_style: Option<String>,
    /// Leftover diagram attrs (warnings / future keys).
    pub extra: AttrMap,
}

impl DiagramMeta {
    pub fn to_render_meta(&self) -> RenderMeta {
        RenderMeta {
            title: self.title.clone(),
            theme: self.theme.clone(),
            render_style: self.render_style.clone(),
        }
    }

    /// Apply profile defaults then author overrides for layout / edge_routing.
    pub fn resolve_algorithms(&self) -> (AlgorithmRef, Option<AlgorithmRef>, Option<DiagramType>) {
        let profile = self.profile.or_else(|| {
            if self.layout.is_none() {
                Some(DiagramType::Flowchart)
            } else {
                None
            }
        });

        let (default_layout, default_routing) = match profile {
            Some(dt) => {
                let p = Profile::for_type(dt);
                (p.default_layout, p.default_edge_routing)
            }
            None => (AlgorithmRef::new("hierarchical"), None),
        };

        let layout = self.layout.clone().unwrap_or(default_layout);
        let edge_routing = self.edge_routing.clone().or(default_routing);
        (layout, edge_routing, profile)
    }
}

/// Expand `@group` endpoints into `group_anchor` nodes + rewrite edges.
pub fn expand_group_frame_sugar(graph: &mut Graph) -> Result<(), ParseError> {
    let _ = graph;
    Err(ParseError::NotImplemented {
        stage: "expand_group_frame_sugar",
        detail: "dsl-spec §7.6 @group expansion pending".into(),
    })
}

/// Lift node/edge structural attrs on the whole graph.
pub fn lift_structural(graph: &mut Graph) -> Result<(), ParseError> {
    graph.lift_all_node_structural_attrs()?;
    graph.lift_all_edge_structural_attrs()?;
    Ok(())
}
