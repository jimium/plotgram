//! Post-lower expands: `@group` sugar, structural attr lift, profile defaults.

use std::collections::BTreeMap;

use plotgram_model::attr::{AttrMap, AttrValue};
use plotgram_model::contract::AlgorithmRef;
use plotgram_model::graph::{Edge, Graph, Group, Node, NodeRole};
use plotgram_model::port::{PortConstraint, Side};
use plotgram_model::profile::{DiagramType, Profile};
use plotgram_model::render::RenderMeta;

use crate::ast::EndpointAst;
use crate::error::ParseError;
use crate::lower::PendingGroupEdge;

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
    /// Non-fatal parse warnings collected during lower/expand.
    pub warnings: Vec<crate::error::ParseWarning>,
}

impl DiagramMeta {
    pub fn to_render_meta(&self) -> RenderMeta {
        RenderMeta {
            title: self.title.clone(),
            theme: self.theme.clone(),
            render_style: self.render_style.clone(),
            extra: self.extra.clone(),
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

/// Expand `@group` endpoints into `group_anchor` nodes + rewrite edges (dsl-spec §7.6.2).
pub fn expand_group_frame_sugar(
    graph: &mut Graph,
    pending: &[PendingGroupEdge],
) -> Result<(), ParseError> {
    if pending.is_empty() {
        return Ok(());
    }

    // Cache: (group_id, side_str, slot) → anchor node id (for reuse)
    let mut anchor_cache: BTreeMap<(String, String, Option<u32>), String> = BTreeMap::new();
    // Collected anchor nodes per group id
    let mut anchors_to_inject: BTreeMap<String, Vec<Node>> = BTreeMap::new();
    // Expanded edges to add at top level
    let mut expanded_edges: Vec<Edge> = Vec::new();
    // §7.6.2 #3: synthesized anchor ids must be globally unique
    let mut used_ids = collect_used_ids(graph);

    for pe in pending {
        let source_id = resolve_endpoint(
            &pe.source,
            "from_side",
            "from_slot",
            &pe.attrs,
            &pe.id,
            &mut anchor_cache,
            &mut anchors_to_inject,
            &mut used_ids,
        )?;
        let target_id = resolve_endpoint(
            &pe.target,
            "to_side",
            "to_slot",
            &pe.attrs,
            &pe.id,
            &mut anchor_cache,
            &mut anchors_to_inject,
            &mut used_ids,
        )?;

        // Build the edge with rewritten endpoints
        let mut attrs = pe.attrs.clone();
        let context = format!("edge `{}`", pe.id);
        let label = crate::lower::take_string_attr(&mut attrs, "label", &context)?;
        let head_label = crate::lower::take_string_attr(&mut attrs, "head_label", &context)?;
        let tail_label = crate::lower::take_string_attr(&mut attrs, "tail_label", &context)?;

        expanded_edges.push(Edge {
            id: pe.id.clone(),
            source: source_id,
            target: target_id,
            arrow: pe.arrow,
            label,
            head_label,
            tail_label,
            from_port: None,
            to_port: None,
            edge_group: None,
            attrs,
        });
    }

    // Inject anchor nodes into their host groups
    for (group_id, anchors) in &anchors_to_inject {
        inject_anchors_into_group(graph, group_id, anchors)?;
    }

    // Add expanded edges at top level, then restore declaration order (§8.1):
    // parser-assigned ids are `e{n}` in declaration order.
    graph.edges.extend(expanded_edges);
    graph.edges.sort_by_key(|e| edge_ordinal(&e.id));

    Ok(())
}

/// Parser-assigned edge ids are `e{n}`; unknown ids sort last (stable).
fn edge_ordinal(id: &str) -> usize {
    id.strip_prefix('e')
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(usize::MAX)
}

/// All node/group ids already present in the graph (recursive).
fn collect_used_ids(graph: &Graph) -> std::collections::HashSet<String> {
    let mut ids = std::collections::HashSet::new();
    for n in &graph.nodes {
        ids.insert(n.id.clone());
    }
    fn walk(groups: &[Group], ids: &mut std::collections::HashSet<String>) {
        for g in groups {
            ids.insert(g.id.clone());
            for n in &g.nodes {
                ids.insert(n.id.clone());
            }
            walk(&g.groups, ids);
        }
    }
    walk(&graph.groups, &mut ids);
    ids
}

/// Resolve one endpoint: if `@group`, create/reuse an anchor node; if node, return id directly.
#[allow(clippy::too_many_arguments)]
fn resolve_endpoint(
    endpoint: &EndpointAst,
    side_key: &'static str,
    slot_key: &'static str,
    edge_attrs: &AttrMap,
    edge_id: &str,
    anchor_cache: &mut BTreeMap<(String, String, Option<u32>), String>,
    anchors_to_inject: &mut BTreeMap<String, Vec<Node>>,
    used_ids: &mut std::collections::HashSet<String>,
) -> Result<String, ParseError> {
    match endpoint {
        EndpointAst::Node(id) => Ok(id.clone()),
        EndpointAst::GroupFrame(gid) => {
            // §7.6.2 rule 2: @group endpoint MUST have corresponding *_side
            let side_atom = edge_attrs
                .get(side_key)
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    ParseError::Semantic(format!(
                        "@{gid}: group-frame endpoint requires `{side_key}` in edge attributes"
                    ))
                })?;
            let side = Side::parse(side_atom).ok_or_else(|| {
                ParseError::Semantic(format!(
                    "`{side_key}`: `{side_atom}` is not one of north/south/east/west"
                ))
            })?;
            let slot = edge_attrs
                .get(slot_key)
                .and_then(|v| v.as_f64())
                .map(|n| {
                    if n >= 0.0 && n.fract() == 0.0 && n <= u32::MAX as f64 {
                        Ok(n as u32)
                    } else {
                        Err(ParseError::Semantic(format!(
                            "`{slot_key}`: `{n}` is not a non-negative integer"
                        )))
                    }
                })
                .transpose()?;

            let cache_key = (gid.clone(), side.as_str().to_string(), slot);
            if let Some(existing_id) = anchor_cache.get(&cache_key) {
                return Ok(existing_id.clone());
            }

            // Synthesize anchor id: ga_{gid}_{side}[_{slot}]; on collision with a
            // declared id, append the edge id suffix (§7.6.2 #3).
            let base_id = match slot {
                Some(s) => format!("ga_{gid}_{}_{s}", side.as_str()),
                None => format!("ga_{gid}_{}", side.as_str()),
            };
            let anchor_id = if used_ids.contains(&base_id) {
                let suffixed = format!("{base_id}_{edge_id}");
                if used_ids.contains(&suffixed) {
                    return Err(ParseError::Semantic(format!(
                        "cannot synthesize unique group_anchor id for @{gid}: both `{base_id}` and `{suffixed}` are taken"
                    )));
                }
                suffixed
            } else {
                base_id
            };
            used_ids.insert(anchor_id.clone());

            let anchor_node = Node {
                id: anchor_id.clone(),
                label: None,
                shape: None,
                role: NodeRole::GroupAnchor,
                host_group: Some(gid.clone()),
                anchor: Some(PortConstraint { side, slot }),
                attrs: AttrMap::new(),
            };

            anchors_to_inject
                .entry(gid.clone())
                .or_default()
                .push(anchor_node);
            anchor_cache.insert(cache_key, anchor_id.clone());

            Ok(anchor_id)
        }
    }
}

/// Inject anchor nodes into the specified group (search recursively).
fn inject_anchors_into_group(
    graph: &mut Graph,
    group_id: &str,
    anchors: &[Node],
) -> Result<(), ParseError> {
    if !inject_into_groups(&mut graph.groups, group_id, anchors) {
        return Err(ParseError::Semantic(format!(
            "cannot inject group_anchor: group `{group_id}` not found"
        )));
    }
    Ok(())
}

fn inject_into_groups(groups: &mut [Group], group_id: &str, anchors: &[Node]) -> bool {
    for g in groups.iter_mut() {
        if g.id == group_id {
            g.nodes.extend(anchors.iter().cloned());
            return true;
        }
        if inject_into_groups(&mut g.groups, group_id, anchors) {
            return true;
        }
    }
    false
}

/// Lift node/edge structural attrs on the whole graph.
pub fn lift_structural(graph: &mut Graph) -> Result<(), ParseError> {
    graph.lift_all_node_structural_attrs()?;
    graph.lift_all_edge_structural_attrs()?;
    validate_group_anchors(graph)?;
    Ok(())
}

/// Validate `group_anchor` placement (dsl-spec §5.7.1).
pub fn validate_group_anchors(graph: &Graph) -> Result<(), ParseError> {
    let group_ids = collect_group_ids(graph);
    validate_anchor_nodes(&graph.nodes, None, &group_ids)?;
    for g in &graph.groups {
        validate_group_anchor_tree(g, &group_ids)?;
    }
    Ok(())
}

fn collect_group_ids(graph: &Graph) -> std::collections::HashSet<String> {
    let mut ids = std::collections::HashSet::new();
    fn walk(groups: &[Group], ids: &mut std::collections::HashSet<String>) {
        for g in groups {
            ids.insert(g.id.clone());
            walk(&g.groups, ids);
        }
    }
    walk(&graph.groups, &mut ids);
    ids
}

fn validate_group_anchor_tree(
    group: &Group,
    group_ids: &std::collections::HashSet<String>,
) -> Result<(), ParseError> {
    validate_anchor_nodes(&group.nodes, Some(&group.id), group_ids)?;
    for child in &group.groups {
        validate_group_anchor_tree(child, group_ids)?;
    }
    Ok(())
}

fn validate_anchor_nodes(
    nodes: &[Node],
    containing_group: Option<&str>,
    group_ids: &std::collections::HashSet<String>,
) -> Result<(), ParseError> {
    for node in nodes {
        if !node.is_group_anchor() {
            continue;
        }
        let host = node.host_group.as_deref().ok_or_else(|| {
            ParseError::Semantic(format!(
                "node `{}`: group_anchor requires `host_group` (dsl-spec §5.7.1)",
                node.id
            ))
        })?;
        if !group_ids.contains(host) {
            return Err(ParseError::Semantic(format!(
                "node `{}`: `host_group: {host}` references unknown group id (dsl-spec §5.7.1)",
                node.id
            )));
        }
        if containing_group != Some(host) {
            return Err(ParseError::Semantic(format!(
                "node `{}`: group_anchor must be a direct member of `host_group: {host}` (dsl-spec §5.7.1)",
                node.id
            )));
        }
    }
    Ok(())
}

/// Expand archetype packs on all nodes (archetype-spec §4: fill-only).
///
/// For each node carrying `archetype:` in attrs, look up the static table and
/// fill `shape` / `variant` / `icon` **only if the axis is not yet set**.
/// Unknown archetype names are silently ignored (diagnose-only, no error).
pub fn expand_archetypes(graph: &mut Graph) {
    expand_archetypes_in_nodes(&mut graph.nodes);
    for g in &mut graph.groups {
        expand_archetypes_in_group(g);
    }
}

fn expand_archetypes_in_group(group: &mut Group) {
    expand_archetypes_in_nodes(&mut group.nodes);
    for g in &mut group.groups {
        expand_archetypes_in_group(g);
    }
}

fn expand_archetypes_in_nodes(nodes: &mut [Node]) {
    use plotgram_model::archetype::archetype_by_id;

    for node in nodes.iter_mut() {
        // Only entity nodes get archetype expansion (anchors have no visual axes).
        if node.is_group_anchor() {
            continue;
        }
        let arch_id = match node.attrs.get("archetype").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => continue,
        };
        let def = match archetype_by_id(&arch_id) {
            Some(d) => d,
            None => continue, // unknown → silently skip
        };

        // Fill shape (Node::shape field)
        if node.shape.is_none() {
            if let Some(s) = def.shape {
                node.shape = Some(s.to_string());
            }
        }
        // Fill variant (attrs)
        if !node.attrs.contains_key("variant") {
            if let Some(var) = def.variant {
                node.attrs.insert("variant".to_string(), AttrValue::Atom(var.to_string()));
            }
        }
        // Fill icon (attrs); explicit `icon: none` counts as "already set"
        if !node.attrs.contains_key("icon") {
            if let Some(icon) = def.icon {
                node.attrs.insert("icon".to_string(), AttrValue::Atom(icon.to_string()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lower::lower;
    use crate::parser::parse_file;

    fn parse_and_expand(source: &str) -> Graph {
        let ast = parse_file(source).unwrap();
        let lowered = lower(&ast).unwrap();
        let mut graph = lowered.graph;
        expand_group_frame_sugar(&mut graph, &lowered.pending_group_edges).unwrap();
        lift_structural(&mut graph).unwrap();
        graph
    }

    #[test]
    fn group_frame_edge_expanded() {
        let graph = parse_and_expand(r#"diagram {
            group fe { node web {} }
            group be { node api {} }
            @fe -> @be { from_side: east to_side: west }
        }"#);

        // Should have 1 edge at top level
        assert_eq!(graph.edges.len(), 1);
        let edge = &graph.edges[0];
        assert!(edge.source.starts_with("ga_fe_east"));
        assert!(edge.target.starts_with("ga_be_west"));

        // Anchor nodes injected into groups
        let fe = &graph.groups[0];
        assert_eq!(fe.nodes.len(), 2); // web + anchor
        let anchor = fe.nodes.iter().find(|n| n.is_group_anchor()).unwrap();
        assert_eq!(anchor.host_group.as_deref(), Some("fe"));
        assert_eq!(anchor.anchor_side(), Some(Side::East));
    }

    #[test]
    fn anchor_reuse_same_group_side() {
        let graph = parse_and_expand(r#"diagram {
            group fe { node web {} }
            group be { node api {} node db {} }
            @fe -> @be { from_side: east to_side: west }
            web -> @be { to_side: west }
        }"#);

        // Both edges targeting @be with to_side: west should share the same anchor
        let be = graph.groups.iter().find(|g| g.id == "be").unwrap();
        let anchor_count = be.nodes.iter().filter(|n| n.is_group_anchor()).count();
        assert_eq!(anchor_count, 1, "anchors with same (group, side, slot) should be reused");
    }

    #[test]
    fn missing_side_error() {
        let ast = parse_file(r#"diagram {
            group fe { node web {} }
            @fe -> web { to_side: north }
        }"#).unwrap();
        let lowered = lower(&ast).unwrap();
        let mut graph = lowered.graph;
        let err = expand_group_frame_sugar(&mut graph, &lowered.pending_group_edges).unwrap_err();
        assert!(matches!(err, ParseError::Semantic(msg) if msg.contains("from_side")));
    }

    #[test]
    fn mixed_endpoint_node_and_group() {
        let graph = parse_and_expand(r#"diagram {
            group be { node api {} }
            node user {}
            user -> @be { to_side: north }
        }"#);

        assert_eq!(graph.edges.len(), 1);
        let edge = &graph.edges[0];
        assert_eq!(edge.source, "user");
        assert!(edge.target.starts_with("ga_be_north"));
    }

    #[test]
    fn no_group_edges_noop() {
        let graph = parse_and_expand("diagram { node a {} node b {} a -> b }");
        assert_eq!(graph.edges.len(), 1);
        assert_eq!(graph.nodes.len(), 2);
    }

    #[test]
    fn validate_group_anchors_rejects_outside_host() {
        let ast = parse_file(r#"diagram {
            group g { node x {} }
            node a { role: group_anchor host_group: g side: north }
        }"#).unwrap();
        let lowered = lower(&ast).unwrap();
        let mut graph = lowered.graph;
        expand_group_frame_sugar(&mut graph, &lowered.pending_group_edges).unwrap();
        let err = lift_structural(&mut graph).unwrap_err();
        assert!(matches!(err, ParseError::Semantic(msg) if msg.contains("direct member")));
    }

    #[test]
    fn resolve_algorithms_default_flowchart() {
        let meta = DiagramMeta::default();
        let (layout, routing, profile) = meta.resolve_algorithms();
        assert_eq!(layout.name, "hierarchical");
        assert!(routing.is_none());
        assert_eq!(profile, Some(DiagramType::Flowchart));
    }

    #[test]
    fn resolve_algorithms_explicit_layout_no_profile() {
        let meta = DiagramMeta {
            layout: Some(AlgorithmRef::new("tree")),
            ..Default::default()
        };
        let (layout, _, profile) = meta.resolve_algorithms();
        assert_eq!(layout.name, "tree");
        assert_eq!(profile, None);
    }

    #[test]
    fn resolve_algorithms_profile_with_override() {
        let meta = DiagramMeta {
            profile: Some(DiagramType::Sequence),
            layout: Some(AlgorithmRef::new("custom")),
            ..Default::default()
        };
        let (layout, _, profile) = meta.resolve_algorithms();
        assert_eq!(layout.name, "custom"); // explicit overrides profile default
        assert_eq!(profile, Some(DiagramType::Sequence));
    }

    // ── archetype expansion ──────────────────────────────────────────────

    fn parse_lift_and_archetype(source: &str) -> Graph {
        let ast = parse_file(source).unwrap();
        let lowered = lower(&ast).unwrap();
        let mut graph = lowered.graph;
        expand_group_frame_sugar(&mut graph, &lowered.pending_group_edges).unwrap();
        lift_structural(&mut graph).unwrap();
        expand_archetypes(&mut graph);
        graph
    }

    #[test]
    fn archetype_fills_shape_variant_icon() {
        let graph = parse_lift_and_archetype(
            r#"diagram { node db { label: "DB" archetype: database } }"#,
        );
        let n = &graph.nodes[0];
        assert_eq!(n.shape.as_deref(), Some("cylinder"));
        assert_eq!(n.attrs.get("variant").and_then(|v| v.as_str()), Some("info"));
        // database has icon: None → no icon filled
        assert!(!n.attrs.contains_key("icon"));
    }

    #[test]
    fn archetype_with_icon_fills_icon() {
        let graph = parse_lift_and_archetype(
            r#"diagram { node svc { label: "Svc" archetype: service } }"#,
        );
        let n = &graph.nodes[0];
        assert_eq!(n.shape.as_deref(), Some("rounded_rect"));
        assert_eq!(n.attrs.get("variant").and_then(|v| v.as_str()), Some("default"));
        assert_eq!(n.attrs.get("icon").and_then(|v| v.as_str()), Some("service"));
    }

    #[test]
    fn archetype_does_not_override_explicit_shape() {
        let graph = parse_lift_and_archetype(
            r#"diagram { node db { archetype: database shape: rounded_rect } }"#,
        );
        let n = &graph.nodes[0];
        // Explicit shape wins over archetype default
        assert_eq!(n.shape.as_deref(), Some("rounded_rect"));
        // variant still filled (not explicitly set)
        assert_eq!(n.attrs.get("variant").and_then(|v| v.as_str()), Some("info"));
    }

    #[test]
    fn archetype_does_not_override_explicit_variant() {
        let graph = parse_lift_and_archetype(
            r#"diagram { node db { archetype: database variant: primary } }"#,
        );
        let n = &graph.nodes[0];
        assert_eq!(n.attrs.get("variant").and_then(|v| v.as_str()), Some("primary"));
    }

    #[test]
    fn archetype_does_not_override_explicit_icon_none() {
        let graph = parse_lift_and_archetype(
            r#"diagram { node svc { archetype: service icon: none } }"#,
        );
        let n = &graph.nodes[0];
        // `icon: none` counts as "already set" — archetype must not overwrite
        assert_eq!(n.attrs.get("icon").and_then(|v| v.as_str()), Some("none"));
    }

    #[test]
    fn archetype_unknown_name_no_error() {
        let graph = parse_lift_and_archetype(
            r#"diagram { node x { archetype: nonexistent } }"#,
        );
        let n = &graph.nodes[0];
        // No expansion, no error
        assert!(n.shape.is_none());
        assert!(!n.attrs.contains_key("variant"));
    }

    #[test]
    fn archetype_in_group_nodes() {
        let graph = parse_lift_and_archetype(r#"diagram {
            group g {
                node db { archetype: cache }
            }
        }"#);
        let n = &graph.groups[0].nodes[0];
        assert_eq!(n.shape.as_deref(), Some("cylinder"));
        assert_eq!(n.attrs.get("icon").and_then(|v| v.as_str()), Some("cache"));
    }

    #[test]
    fn archetype_skips_group_anchor() {
        // Anchors have no archetype attr normally, but ensure they're skipped
        let graph = parse_lift_and_archetype(r#"diagram {
            group fe { node web {} }
            node user {}
            user -> @fe { to_side: north }
        }"#);
        let fe = &graph.groups[0];
        let anchor = fe.nodes.iter().find(|n| n.is_group_anchor()).unwrap();
        assert!(anchor.shape.is_none());
    }
}
