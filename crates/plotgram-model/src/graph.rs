//! Diagram IR: the graph model shared by engine and renderer.
//!
//! Profile expansion fills `LayoutContract` algorithm fields; the `Graph` itself
//! is the structural IR (nodes / edges / groups). No `diagram_type` on `Graph`.
//!
//! Aligned with dsl-spec §5–§7.
//!
//! **Sequence time axis:** when `layout.name == "sequence"`, message time order is
//! the edge declaration order in this graph (see [`Graph::edges_in_declaration_order`]).
//! There is no per-edge `seq` field.

use crate::attr::AttrMap;
use crate::partition::{PartitionCell, PartitionGrid};
use crate::port::{PortConstraint, PortConstraintError, Side, port_constraint};

/// Arrow semantics (dsl-spec §7.2: exactly 3 kinds).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Arrow {
    /// `->` forward / active flow.
    Forward,
    /// `-->` response / return.
    Response,
    /// `<->` bidirectional.
    Bidirectional,
}

/// Structural role of a node (ADR-004 / dsl-spec §5.7).
///
/// Default is a normal content node. [`NodeRole::GroupAnchor`] is an invisible
/// frame attachment point for group-to-group edges — not a business entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeRole {
    /// Ordinary diagram node (default).
    #[default]
    Entity,
    /// Invisible anchor on a group's frame (`host_group` + [`Node::anchor`]).
    GroupAnchor,
}

impl NodeRole {
    /// Parse a DSL `role:` atom. Unknown atoms are errors (closed set for now).
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "entity" => Some(Self::Entity),
            "group_anchor" => Some(Self::GroupAnchor),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Entity => "entity",
            Self::GroupAnchor => "group_anchor",
        }
    }
}

/// Validation / lift errors for node structural fields (ADR-004).
#[derive(Debug, Clone, PartialEq)]
pub enum NodeStructuralError {
    /// `role:` atom outside the closed set.
    InvalidRole { value: String },
    /// `group_anchor` missing `host_group`.
    AnchorMissingHostGroup { node_id: String },
    /// `group_anchor` missing `side` (anchor port).
    AnchorMissingSide { node_id: String },
    /// `entity` node still carries `host_group` / anchor keys after lift.
    EntityHasAnchorFields { node_id: String },
    /// Reused edge-port style errors when lifting node `side` / `slot`.
    Port(PortConstraintError),
}

impl std::fmt::Display for NodeStructuralError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRole { value } => {
                write!(f, "`role`: `{value}` is not one of entity/group_anchor")
            }
            Self::AnchorMissingHostGroup { node_id } => {
                write!(
                    f,
                    "node `{node_id}`: `role: group_anchor` requires `host_group`"
                )
            }
            Self::AnchorMissingSide { node_id } => {
                write!(
                    f,
                    "node `{node_id}`: `role: group_anchor` requires `side`"
                )
            }
            Self::EntityHasAnchorFields { node_id } => {
                write!(
                    f,
                    "node `{node_id}`: `host_group` / `side` / `slot` only valid with `role: group_anchor`"
                )
            }
            Self::Port(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for NodeStructuralError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Port(e) => Some(e),
            _ => None,
        }
    }
}

impl From<PortConstraintError> for NodeStructuralError {
    fn from(value: PortConstraintError) -> Self {
        Self::Port(value)
    }
}

/// A graph node (dsl-spec §5).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Node {
    /// Unique identifier (`[a-z][a-z0-9_]*`).
    pub id: String,
    /// Display label; `None` = unlabeled pure shape.
    pub label: Option<String>,
    /// Explicit rendering shape override (closed set; `None` = resolved via shape chain).
    pub shape: Option<String>,
    /// Structural role (default [`NodeRole::Entity`]).
    #[serde(default)]
    pub role: NodeRole,
    /// Host group id when [`Self::role`] is [`NodeRole::GroupAnchor`].
    #[serde(default)]
    pub host_group: Option<String>,
    /// Attachment on the host group frame (`side` + optional `slot`).
    ///
    /// Required (at least `side`) for [`NodeRole::GroupAnchor`]. Geometry is
    /// derived from the host group's finalized bbox — ink must not invent it.
    #[serde(default)]
    pub anchor: Option<PortConstraint>,
    /// Orthogonal partition cell (`cell_col` / `cell_row` after lift). ADR-008.
    #[serde(default)]
    pub partition_cell: Option<PartitionCell>,
    /// Free-form attributes (`variant`, `icon`, `status`, `style.*`, `meta.*`, …).
    /// Must **not** carry `role` / `host_group` / `side` / `slot` /
    /// `cell_col` / `cell_row` after lift.
    pub attrs: AttrMap,
}

impl Node {
    /// Visual variant from attrs (`variant:`), if present.
    pub fn variant(&self) -> Option<&str> {
        self.attrs.get("variant").and_then(|v| v.as_str())
    }

    /// Whether this node is a [`NodeRole::GroupAnchor`].
    pub fn is_group_anchor(&self) -> bool {
        self.role == NodeRole::GroupAnchor
    }

    /// Lift DSL structural keys from `attrs` into first-class fields, then remove
    /// them from `attrs`. Idempotent if fields already set (fields win; conflicting
    /// attr keys are still stripped).
    ///
    /// Keys handled: `role`, `host_group`, `side`, `slot`, `cell_col`, `cell_row`.
    pub fn lift_structural_attrs(&mut self) -> Result<(), NodeStructuralError> {
        if matches!(self.role, NodeRole::Entity) {
            if let Some(v) = self.attrs.get("role") {
                let atom = v.as_str().unwrap_or_default();
                self.role = NodeRole::parse(atom).ok_or_else(|| {
                    NodeStructuralError::InvalidRole {
                        value: v.to_string(),
                    }
                })?;
            }
        }
        if self.host_group.is_none() {
            if let Some(v) = self.attrs.get("host_group") {
                if let Some(s) = v.as_str() {
                    self.host_group = Some(s.to_string());
                }
            }
        }
        if self.anchor.is_none() {
            self.anchor = port_constraint(&self.attrs, "side", "slot")?;
        }
        if self.partition_cell.is_none() {
            let column = self
                .attrs
                .get("cell_col")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            let row = self
                .attrs
                .get("cell_row")
                .and_then(|v| v.as_str())
                .map(str::to_string);
            if column.is_some() || row.is_some() {
                self.partition_cell = Some(PartitionCell { column, row });
            }
        }
        for k in [
            "role",
            "host_group",
            "side",
            "slot",
            "cell_col",
            "cell_row",
        ] {
            self.attrs.remove(k);
        }
        self.validate_role_fields()
    }

    /// Check role ↔ host_group / anchor consistency (call after lift or when
    /// constructing IR by hand).
    pub fn validate_role_fields(&self) -> Result<(), NodeStructuralError> {
        match self.role {
            NodeRole::Entity => {
                if self.host_group.is_some() || self.anchor.is_some() {
                    return Err(NodeStructuralError::EntityHasAnchorFields {
                        node_id: self.id.clone(),
                    });
                }
                Ok(())
            }
            NodeRole::GroupAnchor => {
                if self.host_group.as_ref().map(|s| s.is_empty()).unwrap_or(true) {
                    return Err(NodeStructuralError::AnchorMissingHostGroup {
                        node_id: self.id.clone(),
                    });
                }
                if self.anchor.is_none() {
                    return Err(NodeStructuralError::AnchorMissingSide {
                        node_id: self.id.clone(),
                    });
                }
                Ok(())
            }
        }
    }

    /// Convenience: host side when this is a group anchor.
    pub fn anchor_side(&self) -> Option<Side> {
        self.anchor.map(|a| a.side)
    }
}

/// An edge between two nodes (dsl-spec §7).
///
/// Parallel edges (same source/target) are distinguished by [`Edge::id`].
///
/// Structural layout fields (`from_port` / `to_port` / `edge_group`) are
/// **first-class** — not read from [`Self::attrs`] by the engine. The parser
/// lifts DSL keys into these fields (see [`Edge::lift_structural_attrs`]).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Edge {
    /// Stable unique id within the graph (parser-assigned; not the DSL surface).
    pub id: String,
    /// Source node id.
    pub source: String,
    /// Target node id.
    pub target: String,
    /// Arrow semantics.
    pub arrow: Arrow,
    /// Mid-edge label.
    pub label: Option<String>,
    /// Label near target (e.g. cardinality "1").
    pub head_label: Option<String>,
    /// Label near source (e.g. cardinality "N").
    pub tail_label: Option<String>,
    /// Author port pin at the source end (`None` = algorithm decides).
    ///
    /// Write: DSL author (via parser) or left unset for layout composition.
    /// Layout composition resolves to [`crate::port::PortRef`] on
    /// [`crate::result::EdgePlacement`]; ink must not invent ports.
    #[serde(default)]
    pub from_port: Option<PortConstraint>,
    /// Author port pin at the target end.
    #[serde(default)]
    pub to_port: Option<PortConstraint>,
    /// Edge-group / bus id: edges sharing the same id may merge into a shared trunk.
    ///
    /// `None` = not grouped. Layout/routing is the writer for geometry; this field
    /// is the author's (or future auto-bundler's) structural hint.
    #[serde(default)]
    pub edge_group: Option<String>,
    /// Free-form attributes (`variant`, `style.*`, `meta.*`, …).
    /// Must **not** carry `from_side` / `edge_group` after lift — those are fields above.
    pub attrs: AttrMap,
}

impl Edge {
    /// Lift DSL structural keys from `attrs` into first-class fields, then remove them
    /// from `attrs`. Idempotent if fields already set (fields win; conflicting attr keys
    /// are still stripped).
    ///
    /// Keys handled: `from_side`, `from_slot`, `to_side`, `to_slot`, `edge_group`.
    pub fn lift_structural_attrs(&mut self) -> Result<(), PortConstraintError> {
        if self.from_port.is_none() {
            self.from_port = port_constraint(&self.attrs, "from_side", "from_slot")?;
        }
        if self.to_port.is_none() {
            self.to_port = port_constraint(&self.attrs, "to_side", "to_slot")?;
        }
        if self.edge_group.is_none() {
            if let Some(v) = self.attrs.get("edge_group") {
                if let Some(s) = v.as_str() {
                    self.edge_group = Some(s.to_string());
                }
            }
        }
        for k in [
            "from_side",
            "from_slot",
            "to_side",
            "to_slot",
            "edge_group",
        ] {
            self.attrs.remove(k);
        }
        Ok(())
    }
}

/// A group container (dsl-spec §6). Recursive: may contain nested groups.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Group {
    /// Unique identifier.
    pub id: String,
    /// Display label; `None` = untitled group.
    pub label: Option<String>,
    /// Group-level attributes (`layout` hint, `style.*`, …).
    pub attrs: AttrMap,
    /// Child nodes.
    pub nodes: Vec<Node>,
    /// Edges whose both endpoints are descendants of this group.
    pub edges: Vec<Edge>,
    /// Nested sub-groups.
    pub groups: Vec<Group>,
}

/// The complete diagram graph model.
///
/// Wrapped in [`crate::contract::LayoutContract`] for the engine.
/// No `diagram_type` here — that stays in the profile/parser layer (ADR-001).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Graph {
    /// Top-level nodes (not inside any group).
    pub nodes: Vec<Node>,
    /// Top-level edges (cross-group or between top-level nodes).
    pub edges: Vec<Edge>,
    /// Top-level groups.
    pub groups: Vec<Group>,
    /// Orthogonal partition grid (swimlanes / matrix). ADR-008.
    #[serde(default)]
    pub partition: Option<PartitionGrid>,
}

impl Graph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            groups: Vec::new(),
            partition: None,
        }
    }

    /// All nodes in declaration order (top-level, then groups depth-first).
    pub fn all_nodes(&self) -> Vec<&Node> {
        let mut out = Vec::with_capacity(self.node_count());
        out.extend(self.nodes.iter());
        for g in &self.groups {
            g.collect_nodes_in_declaration_order(&mut out);
        }
        out
    }

    /// Validate [`Self::partition`] against every node's [`Node::partition_cell`].
    pub fn validate_partition(&self) -> Result<(), crate::partition::PartitionError> {
        crate::partition::validate_graph_partition(self)
    }

    /// Total node count (recursive into groups).
    pub fn node_count(&self) -> usize {
        self.nodes.len() + self.groups.iter().map(|g| g.node_count()).sum::<usize>()
    }

    /// Total edge count (recursive into groups).
    pub fn edge_count(&self) -> usize {
        self.edges.len() + self.groups.iter().map(|g| g.edge_count()).sum::<usize>()
    }

    /// Edges in **declaration order** (top-level vector, then each group in order,
    /// depth-first). For `layout: sequence` this **is** the message time axis —
    /// there is no `Edge::seq` field.
    ///
    /// Product guidance: keep sequence messages at the top level; nested group
    /// messages are allowed by this walk but are not a first-class sequence feature.
    pub fn edges_in_declaration_order(&self) -> Vec<&Edge> {
        let mut out = Vec::with_capacity(self.edge_count());
        out.extend(self.edges.iter());
        for g in &self.groups {
            g.collect_edges_in_declaration_order(&mut out);
        }
        out
    }

    /// Lift structural attrs on every edge (recursive). Call after parse.
    pub fn lift_all_edge_structural_attrs(&mut self) -> Result<(), PortConstraintError> {
        for e in &mut self.edges {
            e.lift_structural_attrs()?;
        }
        for g in &mut self.groups {
            g.lift_all_edge_structural_attrs()?;
        }
        Ok(())
    }

    /// Lift structural attrs on every node (recursive). Call after parse
    /// (and after `@group` sugar expansion into `group_anchor` nodes).
    pub fn lift_all_node_structural_attrs(&mut self) -> Result<(), NodeStructuralError> {
        for n in &mut self.nodes {
            n.lift_structural_attrs()?;
        }
        for g in &mut self.groups {
            g.lift_all_node_structural_attrs()?;
        }
        Ok(())
    }

    /// Look up a node by id (top-level first, then groups in declaration order).
    pub fn find_node(&self, id: &str) -> Option<&Node> {
        if let Some(n) = self.nodes.iter().find(|n| n.id == id) {
            return Some(n);
        }
        for g in &self.groups {
            if let Some(n) = g.find_node(id) {
                return Some(n);
            }
        }
        None
    }

    /// Look up an edge by id (top-level first, then groups in declaration order).
    pub fn find_edge(&self, id: &str) -> Option<&Edge> {
        if let Some(e) = self.edges.iter().find(|e| e.id == id) {
            return Some(e);
        }
        for g in &self.groups {
            if let Some(e) = g.find_edge(id) {
                return Some(e);
            }
        }
        None
    }
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}

impl Group {
    /// Recursive node count within this group.
    pub fn node_count(&self) -> usize {
        self.nodes.len() + self.groups.iter().map(|g| g.node_count()).sum::<usize>()
    }

    /// Recursive edge count within this group.
    pub fn edge_count(&self) -> usize {
        self.edges.len() + self.groups.iter().map(|g| g.edge_count()).sum::<usize>()
    }

    fn collect_nodes_in_declaration_order<'a>(&'a self, out: &mut Vec<&'a Node>) {
        out.extend(self.nodes.iter());
        for g in &self.groups {
            g.collect_nodes_in_declaration_order(out);
        }
    }

    fn collect_edges_in_declaration_order<'a>(&'a self, out: &mut Vec<&'a Edge>) {
        out.extend(self.edges.iter());
        for g in &self.groups {
            g.collect_edges_in_declaration_order(out);
        }
    }

    fn lift_all_edge_structural_attrs(&mut self) -> Result<(), PortConstraintError> {
        for e in &mut self.edges {
            e.lift_structural_attrs()?;
        }
        for g in &mut self.groups {
            g.lift_all_edge_structural_attrs()?;
        }
        Ok(())
    }

    fn lift_all_node_structural_attrs(&mut self) -> Result<(), NodeStructuralError> {
        for n in &mut self.nodes {
            n.lift_structural_attrs()?;
        }
        for g in &mut self.groups {
            g.lift_all_node_structural_attrs()?;
        }
        Ok(())
    }

    /// Look up a node by id within this group subtree.
    pub fn find_node(&self, id: &str) -> Option<&Node> {
        if let Some(n) = self.nodes.iter().find(|n| n.id == id) {
            return Some(n);
        }
        for g in &self.groups {
            if let Some(n) = g.find_node(id) {
                return Some(n);
            }
        }
        None
    }

    /// Look up an edge by id within this group subtree.
    pub fn find_edge(&self, id: &str) -> Option<&Edge> {
        if let Some(e) = self.edges.iter().find(|e| e.id == id) {
            return Some(e);
        }
        for g in &self.groups {
            if let Some(e) = g.find_edge(id) {
                return Some(e);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attr::AttrValue;
    use crate::port::Side;

    fn node(id: &str) -> Node {
        Node {
            id: id.to_string(),
            label: Some(id.to_string()),
            shape: None,
            role: NodeRole::Entity,
            host_group: None,
            anchor: None,
            partition_cell: None,
            attrs: AttrMap::new(),
        }
    }

    fn edge(id: &str, arrow: Arrow) -> Edge {
        Edge {
            id: id.to_string(),
            source: "a".to_string(),
            target: "b".to_string(),
            arrow,
            label: None,
            head_label: None,
            tail_label: None,
            from_port: None,
            to_port: None,
            edge_group: None,
            attrs: AttrMap::new(),
        }
    }

    #[test]
    fn find_node_and_edge_recurse_into_nested_groups() {
        let cases = [
            ("top_node", true, false),
            ("inner_node", true, false),
            ("deep_node", true, false),
            ("missing", false, false),
            ("top_edge", false, true),
            ("inner_edge", false, true),
            ("deep_edge", false, true),
        ];

        let graph = Graph {
            nodes: vec![node("top_node")],
            edges: vec![edge("top_edge", Arrow::Forward)],
            groups: vec![Group {
                id: "g1".to_string(),
                label: Some("G1".to_string()),
                attrs: AttrMap::new(),
                nodes: vec![node("inner_node")],
                edges: vec![edge("inner_edge", Arrow::Response)],
                groups: vec![Group {
                    id: "g2".to_string(),
                    label: Some("G2".to_string()),
                    attrs: AttrMap::new(),
                    nodes: vec![node("deep_node")],
                    edges: vec![edge("deep_edge", Arrow::Bidirectional)],
                    groups: vec![],
                }],
            }],
            partition: None,
        };

        for (id, want_node, want_edge) in cases {
            assert_eq!(graph.find_node(id).is_some(), want_node, "find_node({id})");
            assert_eq!(graph.find_edge(id).is_some(), want_edge, "find_edge({id})");
        }
        assert_eq!(
            graph.find_edge("inner_edge").map(|e| e.arrow),
            Some(Arrow::Response)
        );
        assert_eq!(
            graph.find_edge("deep_edge").map(|e| e.arrow),
            Some(Arrow::Bidirectional)
        );
    }

    #[test]
    fn edges_in_declaration_order_is_sequence_time_axis() {
        let graph = Graph {
            nodes: vec![],
            edges: vec![edge("e0", Arrow::Forward), edge("e1", Arrow::Response)],
            groups: vec![Group {
                id: "g".to_string(),
                label: None,
                attrs: AttrMap::new(),
                nodes: vec![],
                edges: vec![edge("e2", Arrow::Forward)],
                groups: vec![],
            }],
            partition: None,
        };
        let ids: Vec<_> = graph
            .edges_in_declaration_order()
            .iter()
            .map(|e| e.id.as_str())
            .collect();
        assert_eq!(ids, ["e0", "e1", "e2"]);
    }

    #[test]
    fn lift_structural_attrs_to_first_class_fields() {
        let mut e = edge("e", Arrow::Forward);
        e.attrs.insert("from_side".into(), AttrValue::Atom("south".into()));
        e.attrs.insert("from_slot".into(), AttrValue::Num(1.0));
        e.attrs.insert("to_side".into(), AttrValue::Atom("north".into()));
        e.attrs.insert("edge_group".into(), AttrValue::Atom("bus_a".into()));
        e.attrs.insert("style.stroke".into(), AttrValue::Str("#f00".into()));

        e.lift_structural_attrs().unwrap();

        assert_eq!(
            e.from_port,
            Some(PortConstraint {
                side: Side::South,
                slot: Some(1)
            })
        );
        assert_eq!(
            e.to_port,
            Some(PortConstraint {
                side: Side::North,
                slot: None
            })
        );
        assert_eq!(e.edge_group.as_deref(), Some("bus_a"));
        assert!(!e.attrs.contains_key("from_side"));
        assert!(!e.attrs.contains_key("edge_group"));
        assert!(e.attrs.contains_key("style.stroke"));
    }

    #[test]
    fn lift_node_group_anchor_structural_attrs() {
        let mut n = node("ga_fe");
        n.attrs.insert("role".into(), AttrValue::Atom("group_anchor".into()));
        n.attrs
            .insert("host_group".into(), AttrValue::Atom("frontend".into()));
        n.attrs.insert("side".into(), AttrValue::Atom("east".into()));
        n.attrs.insert("slot".into(), AttrValue::Num(0.0));
        n.attrs
            .insert("style.fill".into(), AttrValue::Str("#fff".into()));

        n.lift_structural_attrs().unwrap();

        assert_eq!(n.role, NodeRole::GroupAnchor);
        assert_eq!(n.host_group.as_deref(), Some("frontend"));
        assert_eq!(
            n.anchor,
            Some(PortConstraint {
                side: Side::East,
                slot: Some(0)
            })
        );
        assert!(!n.attrs.contains_key("role"));
        assert!(!n.attrs.contains_key("host_group"));
        assert!(!n.attrs.contains_key("side"));
        assert!(n.attrs.contains_key("style.fill"));
        assert!(n.is_group_anchor());
    }

    #[test]
    fn entity_rejects_anchor_fields() {
        let mut n = node("x");
        n.host_group = Some("g".into());
        assert!(matches!(
            n.validate_role_fields(),
            Err(NodeStructuralError::EntityHasAnchorFields { .. })
        ));
    }

    #[test]
    fn lift_node_partition_cell_attrs() {
        let mut n = node("place_order");
        n.attrs
            .insert("cell_col".into(), AttrValue::Atom("customer".into()));
        n.attrs
            .insert("cell_row".into(), AttrValue::Atom("intake".into()));
        n.attrs
            .insert("style.fill".into(), AttrValue::Str("#fff".into()));

        n.lift_structural_attrs().unwrap();

        assert_eq!(
            n.partition_cell,
            Some(crate::partition::PartitionCell::at("customer", "intake"))
        );
        assert!(!n.attrs.contains_key("cell_col"));
        assert!(!n.attrs.contains_key("cell_row"));
        assert!(n.attrs.contains_key("style.fill"));
    }

    #[test]
    fn group_anchor_requires_host_and_side() {
        let mut n = node("ga");
        n.role = NodeRole::GroupAnchor;
        assert!(matches!(
            n.validate_role_fields(),
            Err(NodeStructuralError::AnchorMissingHostGroup { .. })
        ));
        n.host_group = Some("g".into());
        assert!(matches!(
            n.validate_role_fields(),
            Err(NodeStructuralError::AnchorMissingSide { .. })
        ));
        n.anchor = Some(PortConstraint {
            side: Side::West,
            slot: None,
        });
        n.validate_role_fields().unwrap();
    }
}
