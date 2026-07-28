//! R3-4：weak Super 图构建、放置（Vertical=LK+BK / Horizontal=堆叠）与 expand。

use super::meta::{expand_contraction, ContractionMeta};
use super::weak::{
    self, AlignMode, ArrangementMode, FlowchartIntraGroupLayouter, StackingArrangement,
    UNGROUPED_ID,
};
use crate::ast::{
    ArrowType, AttributeMap, AttributeValue, Diagram, Entity, Identifier, Relation, Span,
};
use crate::layout::algorithm_config::SugiyamaLayoutConfig;
use crate::layout::atlas::plan::Slot;
use crate::layout::kernel::common::divide_and_conquer::{
    CrossGroupEdge, GroupArrangement, GroupTree, IntraGroupLayouter, IntraLayout,
};
use crate::layout::kernel::coordinate::model::CoordinateProblem;
use crate::layout::kernel::layered::coordinate::assign_coordinates_brandes_koepf_with_main_tops;
use crate::layout::kernel::layered::layered_kernel::{LayeredDraft, LayeredKernel};
use crate::layout::kernel::layered::preset;
use crate::layout::NodeLayout;
use std::collections::{BTreeMap, HashMap, HashSet};

/// `contract_weak` 产出：meta + 放置所需的跨组边 / 间距。
pub struct WeakContractBundle {
    pub meta: ContractionMeta,
    pub cross_edges: Vec<CrossGroupEdge>,
    pub gap: f64,
    pub align: AlignMode,
}

/// Super 放置结果。
pub struct WeakPlaceResult {
    pub super_nodes: HashMap<String, NodeLayout>,
    pub draft: Option<LayeredDraft>,
    pub cross_problem: Option<CoordinateProblem>,
}

/// expand + slots 后的 weak 布局核。
pub struct WeakExpandResult {
    pub nodes: HashMap<String, NodeLayout>,
    pub slots: BTreeMap<String, Slot>,
    pub sugiyama_ranks: HashMap<String, usize>,
    pub order: Vec<String>,
    pub mode: ArrangementMode,
    pub canvas_padding: f64,
    pub draft: Option<LayeredDraft>,
    pub cross_problem: Option<CoordinateProblem>,
}

/// 组内 Intra + 选轴 + 拓扑序 → [`ContractionMeta`]。
pub fn contract_weak(diagram: &Diagram, config: SugiyamaLayoutConfig) -> WeakContractBundle {
    let tree = GroupTree::build(diagram);

    let top_groups: Vec<String> = diagram
        .groups
        .iter()
        .filter(|g| g.parent_id.is_none())
        .map(|g| g.id.as_str().to_string())
        .collect();
    let ungrouped: Vec<String> = diagram
        .entities
        .iter()
        .filter(|e| e.group_id.is_none())
        .map(|e| e.id.as_str().to_string())
        .collect();

    let entity_to_group = weak::build_entity_to_top_group_pub(diagram, &top_groups, &tree);
    let layouter = FlowchartIntraGroupLayouter::new(diagram, config);
    let mut intras: BTreeMap<String, IntraLayout> = BTreeMap::new();

    for gid in &top_groups {
        let members = tree.descendant_entities(gid);
        let intra = layouter.layout_intra(gid, &members);
        intras.insert(gid.clone(), intra);
    }
    if !ungrouped.is_empty() {
        let intra = layouter.layout_intra(UNGROUPED_ID, &ungrouped);
        intras.insert(UNGROUPED_ID.to_string(), intra);
    }

    let cross_edges = weak::collect_cross_edges_pub(diagram, &entity_to_group);

    let mut all_group_ids = top_groups.clone();
    if !ungrouped.is_empty() {
        all_group_ids.push(UNGROUPED_ID.to_string());
    }
    let (gap, align, default_mode) = weak::read_arrangement_config_pub(diagram);
    let mode = weak::choose_arrangement_mode_pub(diagram, &intras, gap, default_mode);
    let order = weak::topological_sort_groups_pub(&all_group_ids, &cross_edges);

    let mut entity_to_super = BTreeMap::new();
    for (eid, gid) in &entity_to_group {
        entity_to_super.insert(eid.clone(), gid.clone());
    }
    for eid in &ungrouped {
        entity_to_super.insert(eid.clone(), UNGROUPED_ID.to_string());
    }

    let meta = ContractionMeta {
        intras,
        super_ids: order,
        entity_to_super,
        mode,
    };

    crate::perf_log!(
        "[atlas] weak contract mode={:?} supers={}",
        meta.mode,
        meta.super_ids.len()
    );
    WeakContractBundle {
        meta,
        cross_edges,
        gap,
        align,
    }
}

/// 将顶层组收缩为无组 Super 节点 Diagram。
pub fn build_super_diagram(diagram: &Diagram, meta: &ContractionMeta) -> Diagram {
    let mut entities = Vec::with_capacity(meta.super_ids.len());
    for gid in &meta.super_ids {
        let intra = meta.intras.get(gid);
        let (w, h) = intra
            .map(|i| (i.content_width.max(1.0), i.content_height.max(1.0)))
            .unwrap_or((40.0, 40.0));
        let mut attrs = AttributeMap::default();
        attrs
            .style
            .insert("width".into(), AttributeValue::Number(w));
        attrs
            .style
            .insert("height".into(), AttributeValue::Number(h));
        entities.push(Entity {
            id: Identifier::new_unchecked(gid),
            label: gid.clone(),
            attributes: attrs,
            group_id: None,
            span: Span::dummy(),
        });
    }

    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut relations = Vec::new();
    let push_edge = |from: &str, to: &str, seen: &mut HashSet<(String, String)>, relations: &mut Vec<Relation>| {
        let fs = meta
            .entity_to_super
            .get(from)
            .cloned()
            .unwrap_or_else(|| UNGROUPED_ID.to_string());
        let ts = meta
            .entity_to_super
            .get(to)
            .cloned()
            .unwrap_or_else(|| UNGROUPED_ID.to_string());
        if fs == ts {
            return;
        }
        if !seen.insert((fs.clone(), ts.clone())) {
            return;
        }
        relations.push(Relation {
            from: Identifier::new_unchecked(&fs),
            to: Identifier::new_unchecked(&ts),
            arrow: ArrowType::Active,
            label: None,
            head_label: None,
            tail_label: None,
            attributes: AttributeMap::default(),
            span: Span::dummy(),
        });
    };

    for r in &diagram.relations {
        push_edge(
            r.from.as_str(),
            r.to.as_str(),
            &mut seen,
            &mut relations,
        );
    }
    for c in &diagram.constraints {
        push_edge(
            c.from.as_str(),
            c.to.as_str(),
            &mut seen,
            &mut relations,
        );
    }

    Diagram {
        diagram_type: diagram.diagram_type.clone(),
        attributes: diagram.attributes.clone(),
        entities,
        relations,
        groups: vec![],
        constraints: vec![],
        style_decls: vec![],
        doc_comment: None,
        source_info: Default::default(),
    }
}

/// Vertical：Super Diagram → LayeredKernel + BK；Horizontal：StackingArrangement。
pub fn place_supers(
    diagram: &Diagram,
    bundle: &WeakContractBundle,
) -> WeakPlaceResult {
    match bundle.meta.mode {
        ArrangementMode::Vertical => place_supers_layered(diagram, &bundle.meta),
        ArrangementMode::Horizontal => place_supers_stacking(bundle),
    }
}

fn place_supers_layered(diagram: &Diagram, meta: &ContractionMeta) -> WeakPlaceResult {
    let super_diagram = build_super_diagram(diagram, meta);
    let draft = LayeredKernel::compute(&super_diagram, &preset::FLOWCHART_PRESET);
    let (super_nodes, cross_problem) = assign_coordinates_brandes_koepf_with_main_tops(
        &draft.dag,
        &draft.proper_graph,
        &draft.layers,
        &draft.sizes,
        false,
        &draft.preset,
        &draft.per_layer_gaps,
        draft.has_order_bias,
        &draft.end_ids,
        None,
        true,
    );
    WeakPlaceResult {
        super_nodes,
        draft: Some(draft),
        cross_problem,
    }
}

fn place_supers_stacking(bundle: &WeakContractBundle) -> WeakPlaceResult {
    let arrangement =
        StackingArrangement::new(bundle.gap, bundle.align, ArrangementMode::Horizontal);
    // arrange 要 HashMap；从 BTreeMap 转一次
    let intras_map: HashMap<String, IntraLayout> = bundle
        .meta
        .intras
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let offsets = arrangement.arrange(
        &bundle.meta.super_ids,
        &intras_map,
        &bundle.cross_edges,
    );
    let mut super_nodes = HashMap::new();
    for gid in &bundle.meta.super_ids {
        let (x, y) = offsets.get(gid).copied().unwrap_or((0.0, 0.0));
        let (w, h) = bundle
            .meta
            .intras
            .get(gid)
            .map(|i| (i.content_width, i.content_height))
            .unwrap_or((40.0, 40.0));
        super_nodes.insert(
            gid.clone(),
            NodeLayout {
                x,
                y,
                width: w,
                height: h,
            },
        );
    }
    WeakPlaceResult {
        super_nodes,
        draft: None,
        cross_problem: None,
    }
}

/// expand + 显式 member slots（拓扑序 rank_offset 拼接组内 layers）。
pub fn expand_and_slots(meta: &ContractionMeta, place: WeakPlaceResult) -> WeakExpandResult {
    let nodes = expand_contraction(meta, &place.super_nodes);
    let (slots, sugiyama_ranks) = member_slots_from_meta(meta);
    WeakExpandResult {
        nodes,
        slots,
        sugiyama_ranks,
        order: meta.super_ids.clone(),
        mode: meta.mode,
        canvas_padding: preset::FLOWCHART_PRESET.padding,
        draft: place.draft,
        cross_problem: place.cross_problem,
    }
}

/// R3-4 一站式：contract → place → expand（供 `divide_flowchart_nodes` / solve）。
pub fn solve_weak_contract_expand(
    diagram: &Diagram,
    config: SugiyamaLayoutConfig,
) -> WeakExpandResult {
    let bundle = contract_weak(diagram, config);
    let place = place_supers(diagram, &bundle);
    expand_and_slots(&bundle.meta, place)
}

fn member_slots_from_meta(
    meta: &ContractionMeta,
) -> (BTreeMap<String, Slot>, HashMap<String, usize>) {
    let mut slots = BTreeMap::new();
    let mut sugiyama_ranks = HashMap::new();
    let mut rank_offset = 0usize;
    for gid in &meta.super_ids {
        let Some(intra) = meta.intras.get(gid) else {
            continue;
        };
        for (local_rank, layer) in intra.layers.iter().enumerate() {
            let global_rank = rank_offset + local_rank;
            for (local_order, id) in layer.iter().enumerate() {
                sugiyama_ranks.insert(id.clone(), global_rank);
                slots.insert(
                    id.clone(),
                    Slot {
                        rank: global_rank,
                        order: local_order,
                    },
                );
            }
        }
        rank_offset += intra.layers.len();
    }
    (slots, sugiyama_ranks)
}
