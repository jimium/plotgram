//! Style resolution: (Graph + Theme) → per-element resolved styles.
//!
//! Priority chain: node.attrs["style.*"] > theme.kind_styles[kind] > theme.defaults.node
//! Edge / group: theme.defaults → inline style.*

use std::collections::BTreeMap;

use plotgram_model::attr::{AttrMap, AttrValue};
use plotgram_model::graph::{Arrow, Edge, Graph, Group, Node};

use crate::icons::{self, IconDef};
use crate::theme::{CompiledTheme, KindStyle};

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
    pub shape: String,
    pub text_fill: String,
    pub font_size: f64,
    pub font_weight: Option<String>,
    pub radius: Option<f64>,
    pub stroke_dasharray: Option<String>,
    pub stroke_linecap: Option<String>,
    pub stroke_linejoin: Option<String>,
    pub fill_opacity: Option<f64>,
    pub stroke_opacity: Option<f64>,
    /// Decoration icon (from `icon:` attr or kind inference; shape-compatible).
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
    pub arrow: Arrow,
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
        resolve_group_recursive(group, theme, &mut nodes, &mut edges, &mut groups);
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
    nodes: &mut BTreeMap<String, ResolvedNodeStyle>,
    edges: &mut BTreeMap<String, ResolvedEdgeStyle>,
    groups: &mut BTreeMap<String, ResolvedGroupStyle>,
) {
    groups.insert(group.id.clone(), resolve_group_style(group, theme));
    for node in &group.nodes {
        nodes.insert(node.id.clone(), resolve_node(node, theme));
    }
    for edge in &group.edges {
        edges.insert(edge.id.clone(), resolve_edge(edge, theme));
    }
    for child in &group.groups {
        resolve_group_recursive(child, theme, nodes, edges, groups);
    }
}

/// Resolve a single node's style.
pub fn resolve_node(node: &Node, theme: &CompiledTheme) -> ResolvedNodeStyle {
    // Layer 1: theme defaults (compiled KindStyle)
    // Layer 2: kind_styles — full replace (already materialized against defaults at compile)
    let base = if let Some(kind) = node.kind() {
        theme
            .kind_styles
            .get(kind)
            .unwrap_or(&theme.defaults.node)
    } else {
        &theme.defaults.node
    };
    let mut style = kind_style_to_resolved(base);

    // Layer 3: explicit shape from DSL (`: cylinder`)
    if let Some(shape) = &node.shape {
        style.shape = shape.clone();
    }

    // Layer 4: inline style.* attrs (highest priority)
    apply_inline_node_styles(&mut style, &node.attrs);

    // Icon: resolved last, against the final shape (compatibility check)
    style.icon = icons::resolve_icon(node, &style.shape);

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
        arrow: edge.arrow,
    };

    apply_inline_edge_styles(&mut style, &edge.attrs);

    // `-->` response / return: dashed unless author already set dash
    if edge.arrow == Arrow::Response && style.stroke_dasharray.is_none() {
        style.stroke_dasharray = Some(e.response_dasharray.clone());
    }

    style
}

fn resolve_group_style(group: &Group, theme: &CompiledTheme) -> ResolvedGroupStyle {
    let g = &theme.defaults.group;
    let mut style = ResolvedGroupStyle {
        fill: g.fill.clone(),
        stroke: g.stroke.clone(),
        stroke_width: g.stroke_width,
        text_fill: g.text_fill.clone(),
        radius: g.radius,
        stroke_dasharray: g.stroke_dasharray.clone(),
    };
    apply_inline_group_styles(&mut style, &group.attrs);
    style
}

fn kind_style_to_resolved(ks: &KindStyle) -> ResolvedNodeStyle {
    ResolvedNodeStyle {
        fill: ks.fill.clone(),
        stroke: ks.stroke.clone(),
        stroke_width: ks.stroke_width,
        shape: ks.shape.clone().unwrap_or_else(|| "rounded_rect".to_string()),
        text_fill: ks.text_fill.clone(),
        font_size: ks.font_size,
        font_weight: ks.font_weight.clone(),
        radius: ks.radius,
        stroke_dasharray: ks.stroke_dasharray.clone(),
        stroke_linecap: ks.stroke_linecap.clone(),
        stroke_linejoin: ks.stroke_linejoin.clone(),
        fill_opacity: ks.fill_opacity,
        stroke_opacity: ks.stroke_opacity,
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
                "radius" => {
                    if let Ok(n) = v.parse() {
                        style.radius = Some(n);
                    }
                }
                "dashed" => {
                    if val.as_bool() == Some(true) || v == "true" {
                        style.stroke_dasharray = Some("4,3".to_string());
                    }
                }
                "stroke_dasharray" => style.stroke_dasharray = Some(v),
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
                "stroke" => style.stroke = v,
                "stroke_width" => {
                    if let Ok(n) = v.parse() {
                        style.stroke_width = n;
                    }
                }
                "dashed" => {
                    if val.as_bool() == Some(true) || v == "true" {
                        style.stroke_dasharray = Some("4,3".to_string());
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
                    }
                }
                "stroke_dasharray" => style.stroke_dasharray = Some(v),
                _ => {}
            }
        }
    }
}
