//! Plotgram SVG renderer.
//!
//! Entry points: [`render_svg`] (SVG backend) and [`render_ascii`] (Unicode
//! text backend). Both consume [`plotgram_model::render::RenderInput`]
//! (graph + layout geometry + chrome).
//!
//! Design principles:
//! - Theme = paint (colors, fonts); render_style = brush (standard / sketch)
//! - No `diagrams` section in themes; visual variance driven by `variant`
//! - Icons from explicit `icon:` attribute only (no inference)

pub mod ascii;
pub mod edges;
pub mod group;
pub mod icons;
pub mod outline;
pub mod resolve;
pub mod shapes;
pub mod strategy;
pub mod text;
pub mod theme;
pub mod util;

use plotgram_model::render::RenderInput;

/// Render a complete diagram to Unicode box-drawing text.
pub fn render_ascii(input: &RenderInput) -> String {
    ascii::render_ascii(input)
}

/// Render a complete diagram to SVG.
pub fn render_svg(input: &RenderInput) -> String {
    let strategy = strategy::from_atom(input.meta.render_style.as_deref());
    let theme = theme::load(input.meta.theme.as_deref());
    let resolved = resolve::resolve_graph(&input.graph, &theme);

    let mut svg = SvgBuilder::new(input.layout.canvas_width, input.layout.canvas_height);

    // Canvas background
    svg.canvas_bg(&theme);

    // NOTE: meta.title is intentionally not drawn: layout does not reserve
    // space for it, so painting it would overlap top-most nodes.

    // Groups (behind nodes): paint outer frames first so nested fills stay visible.
    let depths = resolve::group_depth_map(&input.graph);
    let mut group_order: Vec<usize> = (0..input.layout.groups.len()).collect();
    group_order.sort_by_key(|&i| depths.get(&input.layout.groups[i].id).copied().unwrap_or(0));
    for i in group_order {
        group::render_group(&mut svg, &input.layout.groups[i], &resolved, &strategy);
    }

    // Edges (behind nodes)
    for ep in &input.layout.edges {
        edges::render_edge(&mut svg, ep, &resolved, &theme, &strategy);
    }

    // Nodes
    for np in &input.layout.nodes {
        shapes::render_node(&mut svg, np, &resolved, &strategy);
    }

    // Labels (on top)
    for ls in &input.layout.labels {
        text::render_label(&mut svg, ls, &resolved, &theme);
    }

    svg.finish()
}

/// Internal SVG document builder.
pub struct SvgBuilder {
    width: f64,
    height: f64,
    defs: Vec<String>,
    def_keys: std::collections::BTreeSet<String>,
    body: Vec<String>,
}

impl SvgBuilder {
    pub fn new(width: f64, height: f64) -> Self {
        Self {
            width,
            height,
            defs: Vec::new(),
            def_keys: std::collections::BTreeSet::new(),
            body: Vec::new(),
        }
    }

    /// Add a def keyed by id substring (e.g. hatch pattern id), skipping duplicates.
    pub fn add_def_once(&mut self, def: String) {
        // Prefer id="..." when present; else fall back to full string.
        let key = extract_def_id(&def).unwrap_or_else(|| def.clone());
        if self.def_keys.insert(key) {
            self.defs.push(def);
        }
    }

    pub fn add_element(&mut self, elem: String) {
        self.body.push(elem);
    }

    pub fn canvas_bg(&mut self, theme: &theme::CompiledTheme) {
        let bg = &theme.defaults.canvas_background;
        self.body.push(format!(
            r#"<rect width="{}" height="{}" fill="{}"/>"#,
            self.width, self.height, bg
        ));
    }

    pub fn finish(self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" width="{}" height="{}">"#,
            self.width, self.height, self.width, self.height
        ));
        out.push('\n');
        if !self.defs.is_empty() {
            out.push_str("<defs>\n");
            for d in &self.defs {
                out.push_str(d);
                out.push('\n');
            }
            out.push_str("</defs>\n");
        }
        for elem in &self.body {
            out.push_str(elem);
            out.push('\n');
        }
        out.push_str("</svg>\n");
        out
    }
}

fn extract_def_id(def: &str) -> Option<String> {
    let start = def.find("id=\"")? + 4;
    let end = def[start..].find('"')? + start;
    Some(def[start..end].to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use plotgram_model::attr::AttrMap;
    use plotgram_model::geometry::{Point, Rect};
    use plotgram_model::graph::{Arrow, Edge, Graph, Node};
    use plotgram_model::render::RenderMeta;
    use plotgram_model::result::{EdgePath, EdgePlacement, LayoutResult, NodePlacement};

    fn minimal_input() -> RenderInput {
        let node = |id: &str| Node {
            id: id.to_string(),
            label: Some(id.to_string()),
            shape: None,
            role: Default::default(),
            host_group: None,
            anchor: None,
            partition_cell: None,
            attrs: AttrMap::new(),
        };
        let graph = Graph {
            nodes: vec![node("a"), node("b")],
            edges: vec![Edge {
                id: "e1".to_string(),
                source: "a".to_string(),
                target: "b".to_string(),
                arrow: Arrow::Forward,
                label: None,
                head_label: None,
                tail_label: None,
                from_port: None,
                to_port: None,
                weight: None,
                undirected: false,
                attrs: AttrMap::new(),
            }],
            groups: vec![],
            partition: None,
        };
        let layout = LayoutResult {
            nodes: vec![
                NodePlacement { id: "a".to_string(), frame: Rect::new(10.0, 10.0, 80.0, 40.0) },
                NodePlacement { id: "b".to_string(), frame: Rect::new(10.0, 110.0, 80.0, 40.0) },
            ],
            edges: vec![EdgePlacement {
                id: "e1".to_string(),
                source: "a".to_string(),
                target: "b".to_string(),
                path: EdgePath::polyline(vec![
                    Point { x: 50.0, y: 50.0 },
                    Point { x: 50.0, y: 110.0 },
                ]),
                from_port: None,
                to_port: None,
            }],
            groups: vec![],
            labels: vec![],
            canvas_width: 200.0,
            canvas_height: 200.0,
            diagnostics: Default::default(),
        };
        RenderInput {
            graph,
            layout,
            meta: RenderMeta { title: None, theme: None, render_style: None, extra: Default::default() },
        }
    }

    #[test]
    fn render_is_deterministic() {
        use plotgram_model::attr::AttrValue;

        // Sketch mode exercises the nondeterminism-prone paths: per-id jitter
        // seeds, hatch pattern defs, and the def dedup set.
        let mut input = minimal_input();
        input.meta.render_style = Some("sketch".to_string());
        input.graph.nodes[0]
            .attrs
            .insert("style.fill".to_string(), AttrValue::Str("#E3F2FD".to_string()));
        input.graph.nodes[1]
            .attrs
            .insert("style.fill".to_string(), AttrValue::Str("#FFF3E0".to_string()));

        let first_svg = render_svg(&input);
        let first_ascii = render_ascii(&input);
        for _ in 0..2 {
            assert_eq!(render_svg(&input), first_svg, "render_svg must be byte-identical");
            assert_eq!(render_ascii(&input), first_ascii, "render_ascii must be byte-identical");
        }
    }

    #[test]
    fn add_def_once_keeps_first_def_per_id() {
        let mut svg = SvgBuilder::new(100.0, 100.0);
        svg.add_def_once(r#"<pattern id="p1" data="first"/>"#.to_string());
        svg.add_def_once(r#"<pattern id="p1" data="second"/>"#.to_string());
        svg.add_def_once(r#"<pattern id="p2"/>"#.to_string());
        // No id attribute: dedup falls back to the full string
        svg.add_def_once("<filter x=\"0\"/>".to_string());
        svg.add_def_once("<filter x=\"0\"/>".to_string());
        let out = svg.finish();
        assert!(out.contains(r#"data="first""#), "first def per id wins:\n{out}");
        assert!(!out.contains(r#"data="second""#), "same id must not be emitted twice:\n{out}");
        assert!(out.contains(r#"id="p2""#), "distinct ids all emitted:\n{out}");
        assert_eq!(
            out.matches("<filter").count(),
            1,
            "id-less defs dedup by full string:\n{out}"
        );
    }

    #[test]
    fn arrow_marker_def_is_emitted_and_referenced() {
        let svg = render_svg(&minimal_input());
        // Marker definition present in <defs>
        assert!(
            svg.contains(r#"<marker id="arrow-head-normal-"#),
            "missing arrow marker def in SVG:\n{svg}"
        );
        // Edge references the marker
        assert!(
            svg.contains(r##"marker-end="url(#arrow-head-normal-"##),
            "missing marker-end reference in SVG:\n{svg}"
        );
    }

    #[test]
    fn arrow_semantics_and_theme_arrow_style() {
        // Response → dashed; Bidirectional → marker-start as well
        let mut input = minimal_input();
        input.graph.edges[0].arrow = Arrow::Response;
        let svg = render_svg(&input);
        assert!(
            svg.contains(r#"stroke-dasharray="6,4""#),
            "response edge should be dashed:\n{svg}"
        );

        input.graph.edges[0].arrow = Arrow::Bidirectional;
        let svg = render_svg(&input);
        assert!(
            svg.contains(r##"marker-start="url(#arrow-head-"##),
            "bidirectional edge should have marker-start:\n{svg}"
        );

        // Theme with arrow_style: hollow → outlined marker
        input.meta.theme = Some("common.blueprint".to_string());
        let svg = render_svg(&input);
        assert!(
            svg.contains(r#"points="1 1, 10 5, 1 9""#),
            "blueprint theme should emit hollow marker:\n{svg}"
        );

        // Theme with arrow_style: none → no markers anywhere
        input.meta.theme = Some("mindmap.base".to_string());
        let svg = render_svg(&input);
        assert!(
            !svg.contains("<marker") && !svg.contains("marker-end"),
            "arrow_style none should suppress markers:\n{svg}"
        );
    }

    #[test]
    fn per_edge_arrow_color_and_style_overrides() {
        use plotgram_model::attr::AttrValue;

        // Inline style.stroke: arrow head follows the edge color
        let mut input = minimal_input();
        input.graph.edges[0].attrs.insert(
            "style.stroke".to_string(),
            AttrValue::Str("#C62828".to_string()),
        );
        let svg = render_svg(&input);
        assert!(
            svg.contains(r##"d="M 0 0 L 10 5 L 0 10 z" fill="#C62828""##),
            "marker fill should follow inline stroke:\n{svg}"
        );

        // Two edges, one overridden: two distinct marker defs, each referenced
        let mut input = minimal_input();
        input.graph.edges.push(Edge {
            id: "e2".to_string(),
            source: "b".to_string(),
            target: "a".to_string(),
            arrow: Arrow::Forward,
            label: None,
            head_label: None,
            tail_label: None,
            from_port: None,
            to_port: None,
            weight: None,
            undirected: false,
            attrs: {
                let mut a = AttrMap::new();
                a.insert(
                    "style.stroke".to_string(),
                    AttrValue::Str("#C62828".to_string()),
                );
                a
            },
        });
        input.layout.edges.push(EdgePlacement {
            id: "e2".to_string(),
            source: "b".to_string(),
            target: "a".to_string(),
            path: EdgePath::polyline(vec![
                Point { x: 60.0, y: 110.0 },
                Point { x: 60.0, y: 50.0 },
            ]),
            from_port: None,
            to_port: None,
        });
        let svg = render_svg(&input);
        assert_eq!(svg.matches("<marker").count(), 2, "one def per color:\n{svg}");

        // Per-edge arrow_style: none suppresses this edge's markers only
        input.graph.edges[1]
            .attrs
            .insert("style.arrow_style".to_string(), AttrValue::Atom("none".to_string()));
        let svg = render_svg(&input);
        assert_eq!(svg.matches("<marker").count(), 1, "only e1's marker remains:\n{svg}");
        assert_eq!(
            svg.matches("marker-end").count(),
            1,
            "e2 must not reference a marker:\n{svg}"
        );
    }

    #[test]
    fn sketch_strategy_affects_shapes() {
        let mut input = minimal_input();
        input.meta.render_style = Some("sketch".to_string());
        let sketch_svg = render_svg(&input);
        input.meta.render_style = None;
        let standard_svg = render_svg(&input);

        // Sketch: shapes become sampled paths + hatch pattern def
        assert!(
            sketch_svg.contains(r#"<pattern id="hatch-"#),
            "sketch should emit hatch pattern def:\n{sketch_svg}"
        );
        assert!(
            sketch_svg.contains(r##"fill="url(#hatch-"##),
            "sketch node fill should reference hatch pattern:\n{sketch_svg}"
        );
        assert_ne!(sketch_svg, standard_svg, "sketch output must differ from standard");
        // Standard keeps exact primitives (nodes are rects by default)
        assert!(standard_svg.contains("<rect x="), "standard should keep exact rects");
    }

    #[test]
    fn node_icon_is_rendered_from_explicit_attr() {
        use plotgram_model::attr::AttrValue;
        use plotgram_model::result::{LabelOwner, LabelSlot};

        let mut input = minimal_input();
        input.graph.nodes[0]
            .attrs
            .insert("icon".to_string(), AttrValue::Atom("queue".to_string()));
        input.layout.labels.push(LabelSlot {
            owner: LabelOwner::Node("a".to_string()),
            role: None,
            text: "a".to_string(),
            frame: Rect::new(10.0, 10.0, 80.0, 40.0),
        });
        let svg = render_svg(&input);
        assert!(
            svg.contains("<g transform=\"translate("),
            "icon=queue node label should include an icon glyph:\n{svg}"
        );

        // Degrade: icon + label wider than the frame → icon dropped, text kept
        input.layout.labels[0].frame = Rect::new(10.0, 10.0, 24.0, 40.0);
        let svg = render_svg(&input);
        assert!(
            !svg.contains("<g transform=\"translate("),
            "icon must be dropped when it cannot fit the frame:\n{svg}"
        );
        assert!(
            svg.contains(r#"text-anchor="middle""#) && svg.contains(">a</text>"),
            "label text must survive the icon degrade:\n{svg}"
        );
    }

    #[test]
    fn edge_and_group_inline_styles() {
        use plotgram_model::attr::AttrValue;
        use plotgram_model::graph::Group;
        use plotgram_model::result::GroupPlacement;

        let mut input = minimal_input();
        input.graph.edges[0].attrs.insert(
            "style.stroke".to_string(),
            AttrValue::Str("#C62828".to_string()),
        );
        input.graph.edges[0]
            .attrs
            .insert("style.dashed".to_string(), AttrValue::Bool(true));

        input.graph.groups.push(Group {
            id: "g1".to_string(),
            label: Some("G".to_string()),
            attrs: {
                let mut a = AttrMap::new();
                a.insert(
                    "style.fill".to_string(),
                    AttrValue::Str("#FFE0B2".to_string()),
                );
                a.insert("style.fill_opacity".to_string(), AttrValue::Num(0.4));
                a
            },
            nodes: vec![],
            edges: vec![],
            groups: vec![],
        });
        input.layout.groups.push(GroupPlacement {
            id: "g1".to_string(),
            frame: Rect::new(5.0, 5.0, 100.0, 160.0),
        });

        let svg = render_svg(&input);
        assert!(
            svg.contains("stroke=\"#C62828\""),
            "edge style.stroke should appear:\n{svg}"
        );
        assert!(
            svg.contains(r#"stroke-dasharray="4,3""#),
            "edge style.dashed should appear:\n{svg}"
        );
        assert!(
            svg.contains("fill=\"#FFE0B2\""),
            "group style.fill should appear:\n{svg}"
        );
        assert!(
            svg.contains(r#"fill-opacity="0.40""#),
            "group style.fill_opacity should appear:\n{svg}"
        );

        // Theme defaults.group.fill_opacity (blueprint = 0.5)
        input.meta.theme = Some("common.blueprint".to_string());
        input.graph.groups[0].attrs.remove("style.fill_opacity");
        let svg = render_svg(&input);
        assert!(
            svg.contains(r#"fill-opacity="0.50""#),
            "blueprint group fill_opacity should appear:\n{svg}"
        );
    }

    #[test]
    fn nested_group_inner_fill_paints_above_outer() {
        use plotgram_model::graph::Group;
        use plotgram_model::result::GroupPlacement;

        let theme = theme::load(None);
        let outer_fill = theme.group_nest[0].fill.clone();
        let inner_fill = theme.group_nest[1].fill.clone();
        assert_ne!(outer_fill, inner_fill, "precondition: nest steps differ");

        let mut input = minimal_input();
        input.graph.groups.push(Group {
            id: "outer".to_string(),
            label: Some("Outer".to_string()),
            attrs: AttrMap::new(),
            nodes: vec![],
            edges: vec![],
            groups: vec![Group {
                id: "inner".to_string(),
                label: Some("Inner".to_string()),
                attrs: AttrMap::new(),
                nodes: vec![],
                edges: vec![],
                groups: vec![],
            }],
        });
        // finalize post-order: inner before outer in the vec
        input.layout.groups.push(GroupPlacement {
            id: "inner".to_string(),
            frame: Rect::new(30.0, 30.0, 50.0, 50.0),
        });
        input.layout.groups.push(GroupPlacement {
            id: "outer".to_string(),
            frame: Rect::new(10.0, 10.0, 100.0, 100.0),
        });

        let svg = render_svg(&input);
        let outer_pos = svg
            .find(&format!(r#"fill="{outer_fill}""#))
            .expect("outer group fill");
        let inner_pos = svg
            .find(&format!(r#"fill="{inner_fill}""#))
            .expect("inner group fill");
        assert!(
            outer_pos < inner_pos,
            "outer frame must be painted before inner so depth colors stay visible:\n{svg}"
        );
    }

    #[test]
    fn edge_label_bg_and_title_not_drawn() {
        use plotgram_model::attr::AttrValue;
        use plotgram_model::result::{LabelOwner, LabelSlot};

        let mut input = minimal_input();
        input.meta.title = Some("Demo Title".to_string());
        input.graph.edges[0].attrs.insert(
            "style.text_fill".to_string(),
            AttrValue::Str("#AB1234".to_string()),
        );
        input.layout.labels.push(LabelSlot {
            owner: LabelOwner::Edge("e1".to_string()),
            role: Some("mid".to_string()),
            text: "go".to_string(),
            frame: Rect::new(40.0, 70.0, 24.0, 14.0),
        });
        let svg = render_svg(&input);
        // Title is not painted for now: layout reserves no space for it.
        assert!(
            !svg.contains("Demo Title"),
            "meta.title must not be drawn on canvas:\n{svg}"
        );
        // clean-light: label_bg "canvas" resolves to the canvas color #F7F7F8
        assert!(
            svg.contains(r##"rx="2" fill="#F7F7F8" fill-opacity="1.00""##),
            "edge label should get a canvas-colored label_bg rect:\n{svg}"
        );
        assert!(
            svg.contains("fill=\"#AB1234\""),
            "edge style.text_fill should color the label:\n{svg}"
        );
        insta::assert_snapshot!("edge_label_svg", svg);
    }

    #[test]
    fn inline_style_values_are_xml_escaped() {
        use plotgram_model::attr::AttrValue;

        let mut input = minimal_input();
        input.graph.nodes[0].attrs.insert(
            "style.fill".to_string(),
            AttrValue::Str(r#"red" onload="alert(1)"#.to_string()),
        );
        input.graph.edges[0].attrs.insert(
            "style.stroke".to_string(),
            AttrValue::Str("a<b>&c".to_string()),
        );
        let svg = render_svg(&input);
        assert!(
            !svg.contains(r#"" onload=""#),
            "raw quote must not break out of the attribute:\n{svg}"
        );
        assert!(
            svg.contains("red&quot; onload=&quot;alert(1)"),
            "node fill should be escaped verbatim:\n{svg}"
        );
        assert!(
            svg.contains("a&lt;b&gt;&amp;c"),
            "edge stroke should be escaped verbatim:\n{svg}"
        );
    }

    #[test]
    fn minimal_svg_snapshot() {
        let svg = render_svg(&minimal_input());
        insta::assert_snapshot!(svg);
    }
}
