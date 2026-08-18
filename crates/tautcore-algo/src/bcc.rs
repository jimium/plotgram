//! Tarjan biconnected components of an undirected simple graph.
//!
//! Input is `n` vertices `0..n` plus an edge list (declaration order).
//! Self-loops are ignored; parallel edges collapse to one simple edge for the
//! search and are all attributed to that block. Adjacency is sorted by peer
//! index. Isolated vertices become singleton (edgeless) blocks.
//!
//! Block order: min original edge index, then isolated vertices by index.
//! Does not read Graph / DSL / profile.

use std::collections::BTreeMap;
use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BccBlock {
    /// Vertices in the block, sorted.
    pub vertices: Vec<usize>,
    /// Original edge indices in this block, sorted. Empty for an isolated vertex.
    pub edges: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BccForest {
    pub blocks: Vec<BccBlock>,
    /// Vertices that belong to two or more blocks, sorted.
    pub cut_vertices: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BccError {
    Invalid(String),
}

impl fmt::Display for BccError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for BccError {}

/// Compute biconnected components. `edges[i] = (u, v)`.
pub fn biconnected_components(
    n: usize,
    edges: &[(usize, usize)],
) -> Result<BccForest, BccError> {
    for (i, &(u, v)) in edges.iter().enumerate() {
        if u >= n || v >= n {
            return Err(BccError::Invalid(format!(
                "bcc: edge {i} = ({u}, {v}) out of range for n={n}"
            )));
        }
    }

    let mut orig_of_pair: BTreeMap<(usize, usize), Vec<usize>> = BTreeMap::new();
    for (i, &(u, v)) in edges.iter().enumerate() {
        if u == v {
            continue;
        }
        let key = if u < v { (u, v) } else { (v, u) };
        orig_of_pair.entry(key).or_default().push(i);
    }

    let simple: Vec<((usize, usize), Vec<usize>)> = orig_of_pair.into_iter().collect();
    let mut adj: Vec<Vec<(usize, usize)>> = vec![Vec::new(); n];
    for (si, &((u, v), _)) in simple.iter().enumerate() {
        adj[u].push((v, si));
        adj[v].push((u, si));
    }
    for list in adj.iter_mut() {
        list.sort_by_key(|&(v, _)| v);
    }

    let mut disc = vec![0i32; n];
    let mut low = vec![0i32; n];
    let mut parent = vec![None; n];
    let mut time = 0i32;
    let mut stack: Vec<usize> = Vec::new();
    let mut on_stack = vec![false; simple.len()];
    let mut raw_blocks: Vec<(Vec<usize>, Vec<usize>)> = Vec::new();

    for root in 0..n {
        if disc[root] != 0 {
            continue;
        }
        dfs(
            root,
            &adj,
            &simple,
            &mut disc,
            &mut low,
            &mut parent,
            &mut time,
            &mut stack,
            &mut on_stack,
            &mut raw_blocks,
        );
        if !stack.is_empty() {
            pop_block(
                &mut stack,
                &mut on_stack,
                &simple,
                &mut raw_blocks,
                None,
            );
        }
    }

    let mut in_edge_block = vec![false; n];
    let mut blocks: Vec<BccBlock> = raw_blocks
        .into_iter()
        .map(|(mut vertices, mut edges)| {
            vertices.sort_unstable();
            vertices.dedup();
            edges.sort_unstable();
            edges.dedup();
            for &v in &vertices {
                in_edge_block[v] = true;
            }
            BccBlock { vertices, edges }
        })
        .collect();

    for v in 0..n {
        if !in_edge_block[v] {
            blocks.push(BccBlock {
                vertices: vec![v],
                edges: Vec::new(),
            });
        }
    }

    blocks.sort_by(|a, b| {
        let ka = a.edges.first().copied().unwrap_or(usize::MAX / 2 + a.vertices[0]);
        let kb = b.edges.first().copied().unwrap_or(usize::MAX / 2 + b.vertices[0]);
        ka.cmp(&kb)
    });

    let mut block_count = vec![0u32; n];
    for b in &blocks {
        for &v in &b.vertices {
            block_count[v] += 1;
        }
    }
    let cut_vertices: Vec<usize> = (0..n).filter(|&v| block_count[v] >= 2).collect();

    Ok(BccForest {
        blocks,
        cut_vertices,
    })
}

fn dfs(
    u: usize,
    adj: &[Vec<(usize, usize)>],
    simple: &[((usize, usize), Vec<usize>)],
    disc: &mut [i32],
    low: &mut [i32],
    parent: &mut [Option<usize>],
    time: &mut i32,
    stack: &mut Vec<usize>,
    on_stack: &mut [bool],
    blocks: &mut Vec<(Vec<usize>, Vec<usize>)>,
) {
    *time += 1;
    disc[u] = *time;
    low[u] = *time;

    for &(v, si) in &adj[u] {
        if parent[u] == Some(v) {
            continue;
        }
        if disc[v] == 0 {
            parent[v] = Some(u);
            push_simple(stack, on_stack, si);
            dfs(
                v, adj, simple, disc, low, parent, time, stack, on_stack, blocks,
            );
            low[u] = low[u].min(low[v]);
            let is_root = parent[u].is_none();
            if is_root || low[v] >= disc[u] {
                pop_block(stack, on_stack, simple, blocks, Some(si));
            }
        } else if disc[v] < disc[u] {
            push_simple(stack, on_stack, si);
            low[u] = low[u].min(disc[v]);
        }
    }
}

fn push_simple(stack: &mut Vec<usize>, on_stack: &mut [bool], si: usize) {
    if on_stack[si] {
        return;
    }
    on_stack[si] = true;
    stack.push(si);
}

fn pop_block(
    stack: &mut Vec<usize>,
    on_stack: &mut [bool],
    simple: &[((usize, usize), Vec<usize>)],
    blocks: &mut Vec<(Vec<usize>, Vec<usize>)>,
    stop: Option<usize>,
) {
    let mut vertices = Vec::new();
    let mut edges = Vec::new();
    while let Some(si) = stack.pop() {
        on_stack[si] = false;
        let ((u, v), orig) = &simple[si];
        vertices.push(*u);
        vertices.push(*v);
        edges.extend(orig.iter().copied());
        if stop == Some(si) {
            break;
        }
    }
    if vertices.is_empty() {
        return;
    }
    blocks.push((vertices, edges));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verts(b: &BccBlock) -> Vec<usize> {
        b.vertices.clone()
    }

    #[test]
    fn table_bcc_shapes() {
        let cases: &[(&str, usize, &[(usize, usize)], &[&[usize]], &[usize])] = &[
            ("empty", 0, &[], &[], &[]),
            ("isolated", 1, &[], &[&[0]], &[]),
            ("two isolated", 2, &[], &[&[0], &[1]], &[]),
            ("bridge", 2, &[(0, 1)], &[&[0, 1]], &[]),
            ("triangle", 3, &[(0, 1), (1, 2), (2, 0)], &[&[0, 1, 2]], &[]),
            (
                "path3",
                3,
                &[(0, 1), (1, 2)],
                &[&[0, 1], &[1, 2]],
                &[1],
            ),
            (
                "two triangles share vertex",
                5,
                &[(0, 1), (1, 2), (2, 0), (2, 3), (3, 4), (4, 2)],
                &[&[0, 1, 2], &[2, 3, 4]],
                &[2],
            ),
            (
                "two triangles plus bridge",
                6,
                &[
                    (0, 1),
                    (1, 2),
                    (2, 0),
                    (2, 3),
                    (3, 4),
                    (4, 5),
                    (5, 3),
                ],
                &[&[0, 1, 2], &[2, 3], &[3, 4, 5]],
                &[2, 3],
            ),
            ("self-loop ignored", 1, &[(0, 0)], &[&[0]], &[]),
            (
                "parallel edges still one bridge block",
                2,
                &[(0, 1), (1, 0)],
                &[&[0, 1]],
                &[],
            ),
        ];
        for &(name, n, edges, want_blocks, want_cuts) in cases {
            let forest = biconnected_components(n, edges).expect(name);
            let mut got: Vec<Vec<usize>> = forest.blocks.iter().map(verts).collect();
            got.sort();
            let mut want: Vec<Vec<usize>> = want_blocks.iter().map(|b| b.to_vec()).collect();
            want.sort();
            assert_eq!(got, want, "{name} blocks");
            assert_eq!(forest.cut_vertices, want_cuts, "{name} cuts");
            let again = biconnected_components(n, edges).expect(name);
            assert_eq!(forest, again, "{name} not deterministic");
        }
    }

    #[test]
    fn out_of_range_is_invalid() {
        assert!(biconnected_components(1, &[(0, 1)]).is_err());
    }

    #[test]
    fn isolated_plus_edge() {
        let forest = biconnected_components(3, &[(1, 2)]).unwrap();
        let mut got: Vec<Vec<usize>> = forest.blocks.iter().map(verts).collect();
        got.sort();
        assert_eq!(got, vec![vec![0], vec![1, 2]]);
        assert!(forest.cut_vertices.is_empty());
    }
}
