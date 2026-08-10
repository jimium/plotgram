//! Style resolution: (Graph + Theme) → per-element resolved styles.
//!
//! Priority chain (node): compiled_variants[variant] → style.*
//! Priority chain (edge/group): defaults.* → variant cascade → style.*

use std::collections::BTreeMap;

use plotgram_model::attr::{AttrMap, AttrValue};
use plotgram_model::graph::{Arrow, Edge, Graph, Group, Node};
use plotgram_model::NodeShape;

use crate::icons::{self, IconDef};
use crate::theme::{CompiledTheme, VariantStyle};

/// Resolved styles for all elements in a graph, keyed by element id.
#[derive(Debug, Clone)]
pub struct ResolvedGraph {
    pub nodes: BTreeMap<String, ResolvedNodeStyle>,
    pub edges: BTreeMap<String, ResolvedEdgeStyle>,
    pub groups: BTreeMap<String, ResolvedGroupStyle>,
}

/// Fully resolved node style (ready for SVG output).
#[derive(Debug, Clone)]
pub struct ResolvedNodeStyle {
    pub fill: String,
    pub stroke: String,
    pub stroke_width: f64,
    pub shape: NodeShape,
    pub text_fill: String,
    pub font_size: f64,
    pub font_weight: Option<String>,
    pub radius: Option<f64>,
    pub stroke_dasharray: Option<String>,
    pub stroke_linecap: Option<String>,
    pub stroke_linejoin: Option<String>,
    pub fill_opacity: Option<f64>,
    pub stroke_opacity: Option<f64>,
    /// Decoration icon (from explicit `icon:` attr only; shape-compatible).
    pub icon: Option<&'static IconDef>,
}

/// Fully resolved edge style (ready for SVG output).
#[derive(Debug, Clone)]
pub struct ResolvedEdgeStyle {
    pub stroke: String,
    pub stroke_width: f64,
    pub stroke_dasharray: Option<String>,
    pub stroke_linecap: Option<String>,
    pub stroke_linejoin: Option<String>,
    pub stroke_opacity: Option<f64>,
    pub arrow_style: String,
    /// Arrow head paint; follows an inline `style.stroke` override so
    /// author-colored edges get matching heads.
    pub arrow_fill: String,
    pub arrow: Arrow,
    pub text_fill: String,
    pub font_size: f64,
    pub label_bg: Option<String>,
    pub label_bg_opacity: f64,
}

/// Fully resolved group style.
#[derive(Debug, Clone)]
pub struct ResolvedGroupStyle {
    pub fill: String,
    pub stroke: String,
    pub stroke_width: f64,
    pub text_fill: String,
    pub radius: f64,
    pub stroke_dasharray: Option<String>,
    pub fill_opacity: Option<f64>,
}

/// Nesting depth per group id (0 = top-level group).
pub fn group_depth_map(graph: &Graph) -> BTreeMap<String, usize> {
    let mut depths = BTreeMap::new();
    fn walk(groups: &[Group], depth: usize, out: &mut BTreeMap<String, usize>) {
        for group in groups {
            out.insert(group.id.clone(), depth);
            walk(&group.groups, depth + 1, out);
        }
    }
    walk(&graph.groups, 0, &mut depths);
    depths
}

/// Resolve all element styles in a graph.
pub fn resolve_graph(graph: &Graph, theme: &CompiledTheme) -> ResolvedGraph {
    let mut nodes = BTreeMap::new();
    let mut edges = BTreeMap::new();
    let mut groups = BTreeMap::new();

    for node in &graph.nodes {
        nodes.insert(node.id.clone(), resolve_node(node, theme));
    }
    for edge in &graph.edges {
        edges.insert(edge.id.clone(), resolve_edge(edge, theme));
    }
    for group in &graph.groups {
        resolve_group_recursive(group, theme, 0, &mut nodes, &mut edges, &mut groups);
    }

    ResolvedGraph {
        nodes,
        edges,
        groups,
    }
}

fn resolve_group_recursive(
    group: &Group,
    theme: &CompiledTheme,
    depth: usize,
    nodes: &mut BTreeMap<String, ResolvedNodeStyle>,
    edges: &mut BTreeMap<String, ResolvedEdgeStyle>,
    groups: &mut BTreeMap<String, ResolvedGroupStyle>,
) {
    groups.insert(group.id.clone(), resolve_group_style(group, theme, depth));
    for node in &group.nodes {
        nodes.insert(node.id.clone(), resolve_node(node, theme));
    }
    for edge in &group.edges {
        edges.insert(edge.id.clone(), resolve_edge(edge, theme));
    }
    for child in &group.groups {
        resolve_group_recursive(child, theme, depth + 1, nodes, edges, groups);
    }
}

/// Resolve a single node's style.
pub fn resolve_node(node: &Node, theme: &CompiledTheme) -> ResolvedNodeStyle {
    // Shape chain: defaults.node_shape → DSL shape: (archetype/profile filled upstream)
    let shape = node
        .shape
        .clone()
        .unwrap_or_else(|| theme.defaults.node_shape.clone());

    // Paint chain: compiled_variants[variant] → style.*
    let variant_key = node.variant().unwrap_or("default");
    let base = theme
        .compiled_variants
        .get(variant_key)
        .unwrap_or(&theme.defaults.node);
    let mut style = variant_style_to_resolved(base, shape);

    // Inline style.* attrs (highest priority)
    apply_inline_node_styles(&mut style, &node.attrs);

    // Icon: explicit only, against final shape (dsl-spec §14.3.1)
    style.icon = icons::resolve_icon(node, style.shape);

    style
}

fn resolve_edge(edge: &Edge, theme: &CompiledTheme) -> ResolvedEdgeStyle {
    let e = &theme.defaults.edge;
    let mut style = ResolvedEdgeStyle {
        stroke: e.stroke.clone(),
        stroke_width: e.stroke_width,
        stroke_dasharray: None,
        stroke_linecap: e.stroke_linecap.clone(),
        stroke_linejoin: e.stroke_linejoin.clone(),
        stroke_opacity: e.stroke_opacity,
        arrow_style: e.arrow_style.clone(),
        arrow_fill: e.arrow_fill.clone(),
        arrow: edge.arrow,
        text_fill: e.text_fill.clone(),
        font_size: e.font_size,
        label_bg: e.label_bg.clone(),
        label_bg_opacity: e.label_bg_opacity,
    };

    // Variant cascade: pick edge-applicable fields from compiled_variants[v]
    let variant_key = edge
        .attrs
        .get("variant")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    if variant_key != "default" {
        if let Some(vs) = theme.compiled_variants.get(variant_key) {
            // pick: stroke, text_fill (edge applicable keys per style-sheet-spec §5)
            style.stroke = vs.stroke.clone();
            style.text_fill = vs.text_fill.clone();
        }
    }

    apply_inline_edge_styles(&mut style, &edge.attrs);

    // `-->` response / return: dashed unless author already set dash
    if edge.arrow == Arrow::Response && style.stroke_dasharray.is_none() {
        style.stroke_dasharray = Some(e.response_dasharray.clone());
    }

    style
}

fn resolve_group_style(group: &Group, theme: &CompiledTheme, depth: usize) -> ResolvedGroupStyle {
    let nest = theme.group_nest_step(depth);
    let g = &theme.defaults.group;
    let mut style = ResolvedGroupStyle {
        fill: nest.fill.clone(),
        stroke: nest.stroke.clone(),
        stroke_width: nest.stroke_width,
        text_fill: g.text_fill.clone(),
        radius: nest.radius,
        stroke_dasharray: nest.stroke_dasharray.clone(),
        fill_opacity: nest.fill_opacity,
    };

    // Variant cascade: pick group-applicable fields from compiled_variants[v]
    let variant_key = group
        .attrs
        .get("variant")
        .and_then(|v| v.as_str())
        .unwrap_or("default");
    if variant_key != "default" {
        if let Some(vs) = theme.compiled_variants.get(variant_key) {
            // pick: fill, stroke, radius (group applicable keys per style-sheet-spec §5)
            style.fill = vs.fill.clone();
            style.stroke = vs.stroke.clone();
            if let Some(r) = vs.radius {
                style.radius = r;
            }
        }
    }

    apply_inline_group_styles(&mut style, &group.attrs);
    style
}

fn variant_style_to_resolved(vs: &VariantStyle, shape: NodeShape) -> ResolvedNodeStyle {
    ResolvedNodeStyle {
        fill: vs.fill.clone(),
        stroke: vs.stroke.clone(),
        stroke_width: vs.stroke_width,
        shape,
        text_fill: vs.text_fill.clone(),
        font_size: vs.font_size,
        font_weight: vs.font_weight.clone(),
        radius: vs.radius,
        stroke_dasharray: vs.stroke_dasharray.clone(),
        stroke_linecap: vs.stroke_linecap.clone(),
        stroke_linejoin: vs.stroke_linejoin.clone(),
        fill_opacity: vs.fill_opacity,
        stroke_opacity: vs.stroke_opacity,
        icon: None,
    }
}

/// Stringify an inline attr value, XML-escaping it.
///
/// This is the single choke point where user-supplied DSL values enter
/// resolved styles (later interpolated verbatim into SVG attributes), so
/// escaping here covers every emission site. Numeric fields `parse()` the
/// escaped string: any value containing `&<>"` would fail to parse anyway.
fn attr_string(val: &AttrValue) -> String {
    match val {
        AttrValue::Str(s) => crate::util::escape_xml(s),
        AttrValue::Atom(s) => crate::util::escape_xml(s),
        AttrValue::Num(n) => n.to_string(),
        AttrValue::Bool(b) => b.to_string(),
    }
}

fn apply_inline_node_styles(style: &mut ResolvedNodeStyle, attrs: &AttrMap) {
    for (key, val) in attrs {
        if let Some(prop) = key.strip_prefix("style.") {
            let v = attr_string(val);
            match prop {
                "fill" => style.fill = v,
                "stroke" => style.stroke = v,
                "stroke_width" => {
                    if let Ok(n) = v.parse() {
                        style.stroke_width = n;
                    }
                }
                "text_fill" => style.text_fill = v,
                "font_size" => {
                    if let Ok(n) = v.parse() {
                        style.font_size = n;
                    }
                }
                "font_weight" => style.font_weight = Some(v),
                "radius" => {
                    if let Ok(n) = v.parse() {
                        style.radius = Some(n);
                    }
                }
                "dashed" => {
                    if val.as_bool() == Some(true) || v == "true" {
                        style.stroke_dasharray = Some("4,3".to_string());
                    } else if val.as_bool() == Some(false) || v == "false" {
                        style.stroke_dasharray = None;
                    }
                }
                "stroke_dasharray" => style.stroke_dasharray = Some(v),
                "stroke_linecap" => style.stroke_linecap = Some(v),
                "stroke_linejoin" => style.stroke_linejoin = Some(v),
                "fill_opacity" => {
                    if let Ok(n) = v.parse() {
                        style.fill_opacity = Some(n);
                    }
                }
                "stroke_opacity" => {
                    if let Ok(n) = v.parse() {
                        style.stroke_opacity = Some(n);
                    }
                }
                _ => {}
            }
        }
    }
}

fn apply_inline_edge_styles(style: &mut ResolvedEdgeStyle, attrs: &AttrMap) {
    for (key, val) in attrs {
        if let Some(prop) = key.strip_prefix("style.") {
            let v = attr_string(val);
            match prop {
                "stroke" => {
                    // Arrow head follows the edge color unless left to theme
                    style.arrow_fill = v.clone();
                    style.stroke = v;
                }
                "stroke_width" => {
                    if let Ok(n) = v.parse() {
                        style.stroke_width = n;
                    }
                }
                "arrow_style" => style.arrow_style = v,
                "dashed" => {
                    if val.as_bool() == Some(true) || v == "true" {
                        style.stroke_dasharray = Some("4,3".to_string());
                    } else if val.as_bool() == Some(false) || v == "false" {
                        style.stroke_dasharray = None;
                    }
                }
                "stroke_dasharray" => style.stroke_dasharray = Some(v),
                "stroke_linecap" => style.stroke_linecap = Some(v),
                "stroke_linejoin" => style.stroke_linejoin = Some(v),
                "stroke_opacity" => {
                    if let Ok(n) = v.parse() {
                        style.stroke_opacity = Some(n);
                    }
                }
                "text_fill" => style.text_fill = v,
                "font_size" => {
                    if let Ok(n) = v.parse() {
                        style.font_size = n;
                    }
                }
                _ => {}
            }
        }
    }
}

fn apply_inline_group_styles(style: &mut ResolvedGroupStyle, attrs: &AttrMap) {
    for (key, val) in attrs {
        if let Some(prop) = key.strip_prefix("style.") {
            let v = attr_string(val);
            match prop {
                "fill" => style.fill = v,
                "stroke" => style.stroke = v,
                "stroke_width" => {
                    if let Ok(n) = v.parse() {
                        style.stroke_width = n;
                    }
                }
                "text_fill" => style.text_fill = v,
                "radius" => {
                    if let Ok(n) = v.parse() {
                        style.radius = n;
                    }
                }
                "dashed" => {
                    if val.as_bool() == Some(true) || v == "true" {
                        style.stroke_dasharray = Some("4,3".to_string());
                    } else if val.as_bool() == Some(false) || v == "false" {
                        style.stroke_dasharray = None;
                    }
                }
                "stroke_dasharray" => style.stroke_dasharray = Some(v),
                "fill_opacity" => {
                    if let Ok(n) = v.parse() {
                        style.fill_opacity = Some(n);
                    }
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(variant: Option<&str>, shape: Option<NodeShape>, styles: &[(&str, AttrValue)]) -> Node {
        let mut attrs = AttrMap::new();
        if let Some(v) = variant {
            attrs.insert("variant".to_string(), AttrValue::Atom(v.to_string()));
        }
        for (k, v) in styles {
            attrs.insert((*k).to_string(), v.clone());
        }
        Node {
            id: "n".to_string(),
            label: None,
            shape,
            role: Default::default(),
            host_group: None,
            anchor: None,
            partition_cell: None,
            attrs,
        }
    }

    #[test]
    fn nested_groups_use_depth_ladder_before_inline_override() {
        use plotgram_model::graph::{Graph, Group};

        let theme = crate::theme::load(None);
        let outer_fill = theme.group_nest[0].fill.clone();
        let inner_fill = theme.group_nest[1].fill.clone();
        assert_ne!(outer_fill, inner_fill, "precondition: nest steps differ");

        let graph = Graph {
            nodes: vec![],
            edges: vec![],
            groups: vec![Group {
                id: "outer".to_string(),
                label: None,
                attrs: AttrMap::new(),
                nodes: vec![],
                edges: vec![],
                groups: vec![Group {
                    id: "inner".to_string(),
                    label: None,
                    attrs: AttrMap::new(),
                    nodes: vec![],
                    edges: vec![],
                    groups: vec![],
                }],
            }],
            partition: None,
        };

        let resolved = resolve_graph(&graph, &theme);
        assert_eq!(resolved.groups["outer"].fill, outer_fill);
        assert_eq!(resolved.groups["inner"].fill, inner_fill);

        // Inline style.* still wins over the depth ladder.
        let mut graph = graph;
        graph.groups[0].groups[0].attrs.insert(
            "style.fill".to_string(),
            AttrValue::Str("#123456".to_string()),
        );
        let resolved = resolve_graph(&graph, &theme);
        assert_eq!(resolved.groups["inner"].fill, "#123456");
    }

    #[test]
    fn node_style_priority_chain() {
        let theme = crate::theme::load(None);
        let defaults = &theme.defaults.node;
        let primary = &theme.compiled_variants["primary"];
        assert_ne!(
            primary.fill, defaults.fill,
            "precondition: primary variant must differ from defaults for this test"
        );

        // Layer 1: theme defaults only (variant=default equals defaults.node)
        let r = resolve_node(&node(None, None, &[]), &theme);
        assert_eq!(r.fill, defaults.fill);
        assert_eq!(r.shape, theme.defaults.node_shape);

        // Layer 2: variant replaces paint, shape stays independent
        let r = resolve_node(&node(Some("primary"), None, &[]), &theme);
        assert_eq!(r.fill, primary.fill);
        assert_eq!(r.shape, theme.defaults.node_shape);

        // Layer 3: explicit DSL shape overrides default shape, paint from variant
        let r = resolve_node(&node(Some("primary"), Some(NodeShape::Hexagon), &[]), &theme);
        assert_eq!(r.shape, NodeShape::Hexagon);
        assert_eq!(r.fill, primary.fill);

        // Layer 4: inline style.* beats everything below
        let r = resolve_node(
            &node(
                Some("primary"),
                Some(NodeShape::Hexagon),
                &[
                    ("style.fill", AttrValue::Str("#123456".to_string())),
                    ("style.stroke_width", AttrValue::Num(3.0)),
                    ("style.dashed", AttrValue::Bool(true)),
                ],
            ),
            &theme,
        );
        assert_eq!(r.fill, "#123456");
        assert_eq!(r.shape, NodeShape::Hexagon);
        assert_eq!(r.stroke_width, 3.0);
        assert_eq!(r.stroke_dasharray.as_deref(), Some("4,3"));

        // style.dashed: false clears a prior dash (e.g. from muted variant)
        let r = resolve_node(
            &node(
                Some("muted"),
                None,
                &[("style.dashed", AttrValue::Bool(false))],
            ),
            &theme,
        );
        assert!(
            theme.compiled_variants["muted"].stroke_dasharray.is_some(),
            "precondition: muted variant has dash"
        );
        assert_eq!(r.stroke_dasharray, None);

        // Unparseable numerics fall through instead of clobbering lower layers
        let r = resolve_node(
            &node(None, None, &[("style.stroke_width", AttrValue::Str("wide".to_string()))]),
            &theme,
        );
        assert_eq!(r.stroke_width, defaults.stroke_width);
    }

    #[test]
    fn response_edge_dash_respects_author_override() {
        let theme = crate::theme::load(None);
        let edge = |arrow: Arrow, dash: Option<&str>| {
            let mut attrs = AttrMap::new();
            if let Some(d) = dash {
                attrs.insert(
                    "style.stroke_dasharray".to_string(),
                    AttrValue::Str(d.to_string()),
                );
            }
            Edge {
                id: "e".to_string(),
                source: "a".to_string(),
                target: "b".to_string(),
                arrow,
                label: None,
                head_label: None,
                tail_label: None,
                from_port: None,
                to_port: None,
                weight: None,
                undirected: false,
                attrs,
            }
        };

        let resp = theme.defaults.edge.response_dasharray.clone();
        // (arrow, author dash, expected dasharray)
        let cases = [
            (Arrow::Forward, None, None),
            (Arrow::Bidirectional, None, None),
            (Arrow::Response, None, Some(resp.as_str())),
            (Arrow::Response, Some("2,2"), Some("2,2")),
        ];
        for (arrow, dash, expected) in cases {
            let r = resolve_edge(&edge(arrow, dash), &theme);
            assert_eq!(
                r.stroke_dasharray.as_deref(),
                expected,
                "arrow={arrow:?} author_dash={dash:?}"
            );
        }
    }
}
