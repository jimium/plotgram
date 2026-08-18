//! Lower [`crate::ast::FileAst`] → [`tautcore_model::graph::Graph`] + diagram attrs.
//!
//! Does not expand `@group` or profiles — see [`crate::expand`].

use std::collections::HashSet;

use tautcore_model::attr::{AttrMap, AttrValue};
use tautcore_model::contract::AlgorithmRef;
use tautcore_model::graph::{Arrow, Edge, Graph, Group, Node, NodeRole};
use tautcore_model::partition::{PartitionAxis, PartitionGrid};
use tautcore_model::profile::DiagramType;

use crate::ast::*;
use crate::error::{ParseError, ParseWarning};
use crate::expand::DiagramMeta;

/// An edge whose endpoint(s) reference `@group` — deferred to expand phase.
#[derive(Debug, Clone)]
pub struct PendingGroupEdge {
    pub id: String,
    pub source: EndpointAst,
    pub target: EndpointAst,
    pub arrow: Arrow,
    pub attrs: AttrMap,
}

/// Intermediate after AST lowering, before sugar / profile expand.
#[derive(Debug, Clone)]
pub struct Lowered {
    pub graph: Graph,
    pub meta: DiagramMeta,
    /// Edges with `@group` endpoints — expanded later.
    pub pending_group_edges: Vec<PendingGroupEdge>,
}

/// §14: string-typed attr (label / head_label / tail_label).
/// Str or Atom accepted (quoted-form equivalence); Num/Bool are type errors.
pub(crate) fn take_string_attr(
    attrs: &mut AttrMap,
    key: &str,
    context: &str,
) -> Result<Option<String>, ParseError> {
    match attrs.remove(key) {
        Some(AttrValue::Str(s)) => Ok(Some(s)),
        Some(AttrValue::Atom(s)) => Ok(Some(s)),
        Some(other) => Err(ParseError::Semantic(format!(
            "{context}: `{key}` must be a string, got {other}"
        ))),
        None => Ok(None),
    }
}

fn stamp_fragment_attrs(
    attrs: &mut AttrMap,
    path: &[(String, String)],
    label: Option<&str>,
    operand_path: &[Option<u32>],
) {
    let ids: Vec<&str> = path.iter().map(|(id, _)| id.as_str()).collect();
    let kinds: Vec<&str> = path.iter().map(|(_, k)| k.as_str()).collect();
    attrs.insert("fragment".into(), AttrValue::Atom(ids.join(".")));
    if let Some((_, kind)) = path.last() {
        attrs.insert("fragment_kind".into(), AttrValue::Atom(kind.clone()));
    }
    if !kinds.is_empty() {
        attrs.insert(
            "fragment_path_kinds".into(),
            AttrValue::Atom(kinds.join(".")),
        );
    }
    if let Some(l) = label {
        if !attrs.contains_key("fragment_label") {
            attrs.insert("fragment_label".into(), AttrValue::Str(l.to_string()));
        }
    }
    if operand_path.iter().any(Option::is_some) {
        let encoded = operand_path
            .iter()
            .map(|o| match o {
                Some(n) => n.to_string(),
                None => "-".into(),
            })
            .collect::<Vec<_>>()
            .join(".");
        attrs.insert("fragment_path_operands".into(), AttrValue::Atom(encoded));
        if let Some(op) = operand_path.last().copied().flatten() {
            attrs.insert("fragment_operand".into(), AttrValue::Num(f64::from(op)));
        }
    }
}

/// Lower AST to a graph + metadata.
pub fn lower(ast: &FileAst) -> Result<Lowered, ParseError> {
    let mut ctx = LowerCtx::new();
    let mut graph = ctx.lower_items(&ast.diagram.items)?;
    let meta = ctx.lower_meta(&ast.diagram)?;

    // Lower partition block → Graph.partition
    if let Some(partition_ast) = &ast.diagram.partition {
        graph.partition = Some(lower_partition(partition_ast));
    }

    // Self-loop check. Sequence SelfCall is a first-class message (scope.md);
    // honour both the profile flag and an explicit `layout: sequence`.
    let layout_is_sequence = meta.layout.as_ref().is_some_and(|l| l.name == "sequence");
    let allows_self_loop =
        meta.profile.map(|p| p.allows_self_loop()).unwrap_or(false) || layout_is_sequence;
    if !allows_self_loop {
        ctx.check_self_loops(&graph)?;
    }

    Ok(Lowered {
        graph,
        meta,
        pending_group_edges: ctx.pending_group_edges,
    })
}

/// Convert [`PartitionAst`] → model [`PartitionGrid`].
fn lower_partition(ast: &PartitionAst) -> PartitionGrid {
    let mut grid = PartitionGrid::default();
    for axis in &ast.axes {
        let entry = match &axis.label {
            Some(l) => PartitionAxis::with_label(&axis.id, l),
            None => PartitionAxis::new(&axis.id),
        };
        if axis.is_column {
            grid.columns.push(entry);
        } else {
            grid.rows.push(entry);
        }
    }
    grid
}

struct LowerCtx {
    edge_counter: usize,
    pending_group_edges: Vec<PendingGroupEdge>,
    /// All declared node ids (for edge endpoint validation).
    node_ids: HashSet<String>,
    /// All declared group ids.
    group_ids: HashSet<String>,
    /// Combined-fragment ids (must be unique; not in the node/group namespace).
    fragment_ids: HashSet<String>,
}

impl LowerCtx {
    fn new() -> Self {
        Self {
            edge_counter: 0,
            pending_group_edges: Vec::new(),
            node_ids: HashSet::new(),
            group_ids: HashSet::new(),
            fragment_ids: HashSet::new(),
        }
    }

    fn next_edge_id(&mut self) -> String {
        let id = format!("e{}", self.edge_counter);
        self.edge_counter += 1;
        id
    }

    /// First pass: collect all node/group ids for validation.
    fn collect_ids(&mut self, items: &[DiagramItem]) {
        for item in items {
            match item {
                DiagramItem::Node(n) => {
                    self.node_ids.insert(n.id.clone());
                }
                DiagramItem::Group(g) => {
                    self.group_ids.insert(g.id.clone());
                    self.collect_ids(&g.items);
                }
                DiagramItem::Fragment(f) => {
                    self.collect_ids(&f.items);
                    for op in &f.operands {
                        self.collect_ids(&op.items);
                    }
                }
                DiagramItem::Edge(_) => {}
            }
        }
    }

    /// Collect node ids that are descendants of the given items (recursive).
    /// Used to enforce group-internal edges connecting only descendants (dsl-spec §12 #5).
    fn collect_descendant_node_ids(items: &[DiagramItem]) -> HashSet<String> {
        let mut ids = HashSet::new();
        for item in items {
            match item {
                DiagramItem::Node(n) => {
                    ids.insert(n.id.clone());
                }
                DiagramItem::Group(g) => {
                    ids.extend(Self::collect_descendant_node_ids(&g.items));
                }
                DiagramItem::Fragment(f) => {
                    ids.extend(Self::collect_descendant_node_ids(&f.items));
                    for op in &f.operands {
                        ids.extend(Self::collect_descendant_node_ids(&op.items));
                    }
                }
                DiagramItem::Edge(_) => {}
            }
        }
        ids
    }

    /// Validate a bare node-id endpoint: in-scope, known, and not a bare group id.
    /// `allowed` is the local descendant set for group-internal edges, or the global
    /// node id set for top-level edges.
    fn check_endpoint(
        &self,
        nid: &str,
        allowed: &HashSet<String>,
        endpoint_label: &str,
    ) -> Result<(), ParseError> {
        if allowed.contains(nid) {
            Ok(())
        } else if self.group_ids.contains(nid) {
            Err(ParseError::Semantic(format!(
                "edge {endpoint_label} `{nid}`: bare group id cannot be edge endpoint; \
                 use `@{nid}` for group-frame edges (dsl-spec §7.6.3)"
            )))
        } else if self.node_ids.contains(nid) {
            Err(ParseError::Semantic(format!(
                "edge {endpoint_label} `{nid}`: not a descendant of this group; \
                 declare cross-group edges at top level or common ancestor (dsl-spec §12 #5)"
            )))
        } else {
            Err(ParseError::Semantic(format!(
                "edge {endpoint_label} `{nid}`: unknown node id"
            )))
        }
    }

    fn lower_items(&mut self, items: &[DiagramItem]) -> Result<Graph, ParseError> {
        // First collect all ids
        self.collect_ids(items);

        let mut graph = Graph::new();
        for item in items {
            match item {
                DiagramItem::Node(n) => {
                    graph.nodes.push(self.lower_node(n)?);
                }
                DiagramItem::Group(g) => {
                    graph.groups.push(self.lower_group(g)?);
                }
                DiagramItem::Edge(e) => {
                    // Top-level edges: validate against global node set
                    self.lower_edge_into(e, &mut graph.edges, &self.node_ids.clone())?;
                }
                DiagramItem::Fragment(f) => {
                    self.lower_fragment(f, &mut graph.edges, &self.node_ids.clone(), &[], &[])?;
                }
            }
        }
        Ok(graph)
    }

    fn lower_node(&self, n: &NodeAst) -> Result<Node, ParseError> {
        let mut attrs = n.attrs.clone();
        // §14.3: label type = string (Str or Atom accepted; Num/Bool rejected)
        let label = take_string_attr(&mut attrs, "label", &format!("node `{}`", n.id))?;
        // §14.3 / §14.6: shape type = atom from the closed NodeShape set.
        let shape = match attrs.remove("shape") {
            Some(AttrValue::Atom(s)) | Some(AttrValue::Str(s)) => {
                let Some(parsed) = tautcore_model::NodeShape::parse(&s) else {
                    return Err(ParseError::Semantic(format!(
                        "node `{}`: unknown shape `{s}` (dsl-spec §14.6)",
                        n.id
                    )));
                };
                Some(parsed)
            }
            Some(other) => {
                return Err(ParseError::Semantic(format!(
                    "node `{}`: `shape` must be an atom, got {}",
                    n.id, other
                )));
            }
            None => None,
        };

        Ok(Node {
            id: n.id.clone(),
            label,
            shape,
            role: NodeRole::Entity,
            host_group: None,
            anchor: None,
            partition_cell: None,
            attrs,
        })
    }

    fn lower_group(&mut self, g: &GroupAst) -> Result<Group, ParseError> {
        let mut attrs = g.attrs.clone();
        let label = take_string_attr(&mut attrs, "label", &format!("group `{}`", g.id))?;

        // §12 #5: collect descendant node ids for group-local edge validation
        let local_node_ids = Self::collect_descendant_node_ids(&g.items);

        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        let mut groups = Vec::new();

        for item in &g.items {
            match item {
                DiagramItem::Node(n) => {
                    nodes.push(self.lower_node(n)?);
                }
                DiagramItem::Group(child) => {
                    groups.push(self.lower_group(child)?);
                }
                DiagramItem::Edge(e) => {
                    // Group edges: validate against this group's descendants only
                    self.lower_edge_into(e, &mut edges, &local_node_ids)?;
                }
                DiagramItem::Fragment(_) => {
                    return Err(ParseError::Semantic(
                        "fragment blocks cannot appear inside a group".into(),
                    ));
                }
            }
        }

        Ok(Group {
            id: g.id.clone(),
            label,
            attrs,
            nodes,
            edges,
            groups,
        })
    }

    /// Lower an edge. If it has `@group` endpoints, defer to pending_group_edges.
    /// `scope` = the set of valid node ids for this edge's context (global or group-local).
    fn lower_edge_into(
        &mut self,
        e: &EdgeAst,
        edges: &mut Vec<Edge>,
        scope: &HashSet<String>,
    ) -> Result<(), ParseError> {
        let has_group_endpoint = matches!(
            (&e.source, &e.target),
            (EndpointAst::GroupFrame(_), _) | (_, EndpointAst::GroupFrame(_))
        );

        let edge_id = self.next_edge_id();

        if has_group_endpoint {
            // Validate group ids exist
            if let EndpointAst::GroupFrame(gid) = &e.source {
                if !self.group_ids.contains(gid) {
                    return Err(ParseError::Semantic(format!(
                        "@{gid}: unknown group id (not declared)"
                    )));
                }
            }
            if let EndpointAst::GroupFrame(gid) = &e.target {
                if !self.group_ids.contains(gid) {
                    return Err(ParseError::Semantic(format!(
                        "@{gid}: unknown group id (not declared)"
                    )));
                }
            }
            // Validate node endpoints (respect group-internal scope, dsl-spec §12 #5)
            if let EndpointAst::Node(nid) = &e.source {
                self.check_endpoint(nid, scope, "source")?;
            }
            if let EndpointAst::Node(nid) = &e.target {
                self.check_endpoint(nid, scope, "target")?;
            }

            self.pending_group_edges.push(PendingGroupEdge {
                id: edge_id,
                source: e.source.clone(),
                target: e.target.clone(),
                arrow: e.arrow,
                attrs: e.attrs.clone(),
            });
            return Ok(());
        }

        // Normal edge: both endpoints are node ids
        let source = match &e.source {
            EndpointAst::Node(id) => id.clone(),
            EndpointAst::GroupFrame(_) => unreachable!(),
        };
        let target = match &e.target {
            EndpointAst::Node(id) => id.clone(),
            EndpointAst::GroupFrame(_) => unreachable!(),
        };

        // Validate endpoints exist (and are in scope for group-internal edges)
        self.check_endpoint(&source, scope, "source")?;
        self.check_endpoint(&target, scope, "target")?;

        let mut attrs = e.attrs.clone();
        let context = format!("edge `{edge_id}`");
        let label = take_string_attr(&mut attrs, "label", &context)?;
        let head_label = take_string_attr(&mut attrs, "head_label", &context)?;
        let tail_label = take_string_attr(&mut attrs, "tail_label", &context)?;

        edges.push(Edge {
            id: edge_id,
            source,
            target,
            arrow: e.arrow,
            label,
            head_label,
            tail_label,
            from_port: None,
            to_port: None,
            weight: None,
            undirected: false,
            attrs,
        });

        Ok(())
    }

    fn lower_fragment(
        &mut self,
        frag: &FragmentAst,
        edges: &mut Vec<Edge>,
        scope: &HashSet<String>,
        prefix: &[(String, String)],
        prefix_ops: &[Option<u32>],
    ) -> Result<(), ParseError> {
        if !self.fragment_ids.insert(frag.id.clone()) {
            return Err(ParseError::Semantic(format!(
                "duplicate fragment id `{}`",
                frag.id
            )));
        }
        let mut path: Vec<(String, String)> = prefix.to_vec();
        path.push((frag.id.clone(), frag.kind.clone()));
        let has_else = !frag.operands.is_empty();
        let label = frag
            .attrs
            .get("label")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let mut main_ops = prefix_ops.to_vec();
        main_ops.push(if has_else { Some(0) } else { None });
        self.lower_fragment_items(
            &frag.items,
            edges,
            scope,
            &path,
            label.as_deref(),
            &main_ops,
        )?;
        for (i, op) in frag.operands.iter().enumerate() {
            let mut else_ops = prefix_ops.to_vec();
            else_ops.push(Some((i + 1) as u32));
            self.lower_fragment_items(&op.items, edges, scope, &path, None, &else_ops)?;
        }
        Ok(())
    }

    fn lower_fragment_items(
        &mut self,
        items: &[DiagramItem],
        edges: &mut Vec<Edge>,
        scope: &HashSet<String>,
        path: &[(String, String)],
        label: Option<&str>,
        operand_path: &[Option<u32>],
    ) -> Result<(), ParseError> {
        for item in items {
            match item {
                DiagramItem::Edge(e) => {
                    let mut stamped = e.clone();
                    stamp_fragment_attrs(&mut stamped.attrs, path, label, operand_path);
                    self.lower_edge_into(&stamped, edges, scope)?;
                }
                DiagramItem::Fragment(child) => {
                    self.lower_fragment(child, edges, scope, path, operand_path)?;
                }
                DiagramItem::Node(n) => {
                    let fid = path.last().map(|(id, _)| id.as_str()).unwrap_or("?");
                    return Err(ParseError::Semantic(format!(
                        "fragment `{fid}`: node `{}` is not allowed inside a fragment",
                        n.id
                    )));
                }
                DiagramItem::Group(g) => {
                    return Err(ParseError::Semantic(format!(
                        "fragment: group `{}` is not allowed inside a fragment",
                        g.id
                    )));
                }
            }
        }
        Ok(())
    }

    fn lower_meta(&self, diagram: &DiagramAst) -> Result<DiagramMeta, ParseError> {
        let mut meta = DiagramMeta::default();

        for (key, value) in &diagram.attrs {
            match key.as_str() {
                "profile" => {
                    let atom = value.as_str().unwrap_or_default();
                    let dt =
                        DiagramType::from_str(atom).ok_or_else(|| ParseError::UnknownProfile {
                            value: atom.to_string(),
                        })?;
                    meta.profile = Some(dt);
                }
                "title" => {
                    meta.title = value.as_str().map(|s| s.to_string());
                }
                "theme" => {
                    meta.theme = value.as_str().map(|s| s.to_string());
                }
                "render_style" => {
                    meta.render_style = value.as_str().map(|s| s.to_string());
                }
                _ if key.starts_with("meta.") => {
                    meta.extra.insert(key.clone(), value.clone());
                }
                _ => {
                    meta.warnings
                        .push(ParseWarning::unknown_diagram_key(key.clone()));
                    meta.extra.insert(key.clone(), value.clone());
                }
            }
        }

        if let Some(layout_ast) = &diagram.layout {
            meta.layout = Some(AlgorithmRef {
                name: layout_ast.name.clone(),
                options: layout_ast.options.clone(),
            });
        }
        if let Some(routing_ast) = &diagram.edge_routing {
            meta.edge_routing = Some(AlgorithmRef {
                name: routing_ast.name.clone(),
                options: routing_ast.options.clone(),
            });
        }

        Ok(meta)
    }

    fn check_self_loops(&self, graph: &Graph) -> Result<(), ParseError> {
        for e in &graph.edges {
            if e.source == e.target {
                return Err(ParseError::Semantic(format!(
                    "self-loop on `{}` is not allowed (profile does not permit it)",
                    e.source
                )));
            }
        }
        for g in &graph.groups {
            self.check_self_loops_in_group(g)?;
        }
        Ok(())
    }

    fn check_self_loops_in_group(&self, group: &Group) -> Result<(), ParseError> {
        for e in &group.edges {
            if e.source == e.target {
                return Err(ParseError::Semantic(format!(
                    "self-loop on `{}` is not allowed (profile does not permit it)",
                    e.source
                )));
            }
        }
        for g in &group.groups {
            self.check_self_loops_in_group(g)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_file;

    fn lower_ok(source: &str) -> Lowered {
        let ast = parse_file(source).unwrap();
        lower(&ast).unwrap()
    }

    fn lower_err(source: &str) -> ParseError {
        let ast = parse_file(source).unwrap();
        lower(&ast).unwrap_err()
    }

    #[test]
    fn minimal_graph() {
        let lowered = lower_ok("diagram { node a {} node b {} a -> b }");
        assert_eq!(lowered.graph.nodes.len(), 2);
        assert_eq!(lowered.graph.edges.len(), 1);
        assert_eq!(lowered.graph.edges[0].source, "a");
        assert_eq!(lowered.graph.edges[0].target, "b");
        assert_eq!(lowered.graph.edges[0].id, "e0");
    }

    #[test]
    fn node_label_and_shape_lifted() {
        let lowered = lower_ok(r#"diagram { node x { label: "Hello", shape: diamond } }"#);
        let n = &lowered.graph.nodes[0];
        assert_eq!(n.label.as_deref(), Some("Hello"));
        assert_eq!(n.shape, Some(tautcore_model::NodeShape::Diamond));
        assert!(!n.attrs.contains_key("label"));
        assert!(!n.attrs.contains_key("shape"));
    }

    #[test]
    fn group_with_members() {
        let lowered = lower_ok(
            r#"diagram {
            group g1 {
                label: "G1"
                node a { label: "A" }
                node b {}
                a -> b
            }
        }"#,
        );
        assert_eq!(lowered.graph.groups.len(), 1);
        let g = &lowered.graph.groups[0];
        assert_eq!(g.label.as_deref(), Some("G1"));
        assert_eq!(g.nodes.len(), 2);
        assert_eq!(g.edges.len(), 1);
    }

    #[test]
    fn profile_parsed() {
        let lowered = lower_ok("diagram { profile: architecture node a {} }");
        assert_eq!(lowered.meta.profile, Some(DiagramType::Architecture));
    }

    #[test]
    fn unknown_profile_error() {
        let err = lower_err("diagram { profile: unknown_thing node a {} }");
        assert!(matches!(err, ParseError::UnknownProfile { .. }));
    }

    #[test]
    fn layout_algorithm_config() {
        let lowered =
            lower_ok("diagram { layout: hierarchical { direction: top-to-bottom } node a {} }");
        let layout = lowered.meta.layout.unwrap();
        assert_eq!(layout.name, "hierarchical");
        assert_eq!(
            layout.options.get("direction"),
            Some(&AttrValue::Atom("top-to-bottom".into()))
        );
    }

    #[test]
    fn edge_label_lifted() {
        let lowered =
            lower_ok(r#"diagram { node a {} node b {} a -> b { label: "req", head_label: "1" } }"#);
        let e = &lowered.graph.edges[0];
        assert_eq!(e.label.as_deref(), Some("req"));
        assert_eq!(e.head_label.as_deref(), Some("1"));
        assert!(!e.attrs.contains_key("label"));
        assert!(!e.attrs.contains_key("head_label"));
    }

    #[test]
    fn unknown_node_endpoint_error() {
        let err = lower_err("diagram { node a {} a -> missing }");
        assert!(matches!(err, ParseError::Semantic(msg) if msg.contains("missing")));
    }

    #[test]
    fn group_edge_deferred() {
        let lowered = lower_ok(
            r#"diagram {
            group fe { node web {} }
            group be { node api {} }
            @fe -> @be { from_side: east, to_side: west }
        }"#,
        );
        assert_eq!(lowered.graph.edges.len(), 0);
        assert_eq!(lowered.pending_group_edges.len(), 1);
        let pe = &lowered.pending_group_edges[0];
        assert!(matches!(&pe.source, EndpointAst::GroupFrame(id) if id == "fe"));
    }

    #[test]
    fn self_loop_rejected_by_default() {
        // §4.2 算法默认 ≠ 约束默认：未显式声明 profile 时禁止自环；
        // architecture 也显式禁止。
        let err = lower_err("diagram { profile: architecture node a {} a -> a }");
        assert!(matches!(err, ParseError::Semantic(msg) if msg.contains("self-loop")));
    }

    #[test]
    fn self_loop_allowed_flowchart() {
        let lowered = lower_ok("diagram { profile: flowchart node a {} a -> a }");
        assert_eq!(lowered.graph.edges.len(), 1);
    }

    #[test]
    fn self_call_allowed_sequence() {
        let lowered = lower_ok("diagram { profile: sequence node a {} a -> a }");
        assert_eq!(lowered.graph.edges.len(), 1);
        let explicit = lower_ok("diagram { layout: sequence node a {} a -> a }");
        assert_eq!(explicit.graph.edges.len(), 1);
    }

    #[test]
    fn edge_ids_sequential() {
        let lowered = lower_ok("diagram { node a {} node b {} node c {} a -> b b -> c a -> c }");
        let ids: Vec<_> = lowered.graph.edges.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["e0", "e1", "e2"]);
    }
}
