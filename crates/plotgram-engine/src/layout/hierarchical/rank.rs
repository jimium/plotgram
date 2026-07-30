//! Longest-path style ranks on the DAG (ignores back-edges for ranking).

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use plotgram_engine_api::LayoutError;
use plotgram_model::graph::Graph;

/// node id → rank (0 = sources).
pub type RankMap = BTreeMap<String, u32>;

pub fn assign_ranks(graph: &Graph) -> Result<RankMap, LayoutError> {
    let ids = graph.all_node_ids();
    if ids.is_empty() {
        return Ok(RankMap::new());
    }

    let id_set: BTreeSet<String> = ids.iter().cloned().collect();
    let mut succ: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut pred_count: BTreeMap<String, u32> = BTreeMap::new();
    for id in &ids {
        succ.entry(id.clone()).or_default();
        pred_count.entry(id.clone()).or_insert(0);
    }

    for e in graph.edges_in_declaration_order() {
        if !id_set.contains(&e.source) || !id_set.contains(&e.target) {
            continue;
        }
        if e.source == e.target {
            continue; // self-loops do not affect rank
        }
        succ.get_mut(&e.source).unwrap().push(e.target.clone());
        *pred_count.get_mut(&e.target).unwrap() += 1;
    }

    // Kahn-style layering: sources get 0; each node rank = max(pred)+1 when all preds done.
    // For cycles, remaining nodes get max_rank+1 in declaration order.
    let mut ranks = RankMap::new();
    let mut pending_pred = pred_count.clone();
    let mut queue: VecDeque<String> = ids
        .iter()
        .filter(|id| pending_pred.get(*id).copied().unwrap_or(0) == 0)
        .cloned()
        .collect();

    // Stable: declaration order within same ready set
    let order_index: BTreeMap<String, usize> = ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.clone(), i))
        .collect();
    queue
        .make_contiguous()
        .sort_by_key(|id| order_index[id]);

    while let Some(u) = queue.pop_front() {
        let r = ranks.get(&u).copied().unwrap_or(0);
        ranks.insert(u.clone(), r);
        let mut nexts = succ.get(&u).cloned().unwrap_or_default();
        nexts.sort_by_key(|id| order_index[id]);
        for v in nexts {
            let pr = pending_pred.get_mut(&v).unwrap();
            *pr = pr.saturating_sub(1);
            let cand = r + 1;
            ranks
                .entry(v.clone())
                .and_modify(|old| *old = (*old).max(cand))
                .or_insert(cand);
            if *pr == 0 && !queue.iter().any(|x| x == &v) {
                queue.push_back(v);
                queue
                    .make_contiguous()
                    .sort_by_key(|id| order_index[id]);
            }
        }
    }

    // Nodes in cycles / unreached: assign after max
    let max_r = ranks.values().copied().max().unwrap_or(0);
    let mut extra = max_r;
    for id in &ids {
        if !ranks.contains_key(id) {
            extra += 1;
            ranks.insert(id.clone(), extra);
        }
    }

    Ok(ranks)
}
