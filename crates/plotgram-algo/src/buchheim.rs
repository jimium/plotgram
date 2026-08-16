//! Buchheim–Jünger–Leipert linear-time tree placement (centers on the x-axis).
//!
//! Input is an index tree (`children[i]` in left-to-right order) plus node
//! widths. Sibling distance is `w_i/2 + w_j/2 + sibling_gap`. Output is a
//! center-x per node; y is the caller's job (layered placers).
//!
//! Implements thread / default_ancestor / shift-change (reference 05 §1.3).
//! Does not read Graph / DSL / profile.

use std::fmt;

/// One rooted tree as parallel arrays. Nodes are `0..n`; `root` has no parent.
#[derive(Debug, Clone)]
pub struct BuchheimTree {
    pub root: usize,
    /// `children[i]` is left-to-right child order (stable).
    pub children: Vec<Vec<usize>>,
    pub widths: Vec<f64>,
    pub sibling_gap: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuchheimError {
    Invalid(String),
}

impl fmt::Display for BuchheimError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for BuchheimError {}

/// Place node **centers** on x. Values may be negative; the caller translates.
pub fn place(tree: &BuchheimTree) -> Result<Vec<f64>, BuchheimError> {
    let n = validate(tree)?;
    if n == 0 {
        return Ok(Vec::new());
    }
    let mut nodes: Vec<Node> = (0..n)
        .map(|i| Node {
            width: tree.widths[i],
            ..Node::new(i)
        })
        .collect();
    for (p, kids) in tree.children.iter().enumerate() {
        for (k, &c) in kids.iter().enumerate() {
            nodes[c].parent = Some(p);
            nodes[c].child_index = k;
        }
    }
    first_walk(tree.root, tree, &mut nodes);
    let mut x = vec![0.0; n];
    second_walk(tree.root, tree, &nodes, 0.0, &mut x);
    Ok(x)
}

fn validate(tree: &BuchheimTree) -> Result<usize, BuchheimError> {
    let n = tree.children.len();
    if tree.widths.len() != n {
        return Err(BuchheimError::Invalid(format!(
            "buchheim: widths len {} != children len {n}",
            tree.widths.len()
        )));
    }
    if n == 0 {
        return Ok(0);
    }
    if tree.root >= n {
        return Err(BuchheimError::Invalid(format!(
            "buchheim: root {} out of range for n={n}",
            tree.root
        )));
    }
    if tree.sibling_gap < 0.0 || !tree.sibling_gap.is_finite() {
        return Err(BuchheimError::Invalid(
            "buchheim: sibling_gap must be finite and >= 0".into(),
        ));
    }
    let mut seen_as_child = vec![false; n];
    for (p, kids) in tree.children.iter().enumerate() {
        if tree.widths[p] < 0.0 || !tree.widths[p].is_finite() {
            return Err(BuchheimError::Invalid(format!(
                "buchheim: width[{p}] must be finite and >= 0"
            )));
        }
        for &c in kids {
            if c >= n {
                return Err(BuchheimError::Invalid(format!(
                    "buchheim: child {c} of {p} out of range"
                )));
            }
            if c == p {
                return Err(BuchheimError::Invalid(format!(
                    "buchheim: node {p} is its own child"
                )));
            }
            if seen_as_child[c] {
                return Err(BuchheimError::Invalid(format!(
                    "buchheim: node {c} has two parents"
                )));
            }
            seen_as_child[c] = true;
        }
    }
    if seen_as_child[tree.root] {
        return Err(BuchheimError::Invalid(format!(
            "buchheim: root {} has a parent",
            tree.root
        )));
    }
    Ok(n)
}

#[derive(Clone)]
struct Node {
    parent: Option<usize>,
    child_index: usize,
    width: f64,
    prelim: f64,
    modifier: f64,
    shift: f64,
    change: f64,
    thread: Option<usize>,
    ancestor: usize,
}

impl Node {
    fn new(i: usize) -> Self {
        Self {
            parent: None,
            child_index: 0,
            width: 0.0,
            prelim: 0.0,
            modifier: 0.0,
            shift: 0.0,
            change: 0.0,
            thread: None,
            ancestor: i,
        }
    }
}

fn distance(a: usize, b: usize, nodes: &[Node], gap: f64) -> f64 {
    nodes[a].width / 2.0 + nodes[b].width / 2.0 + gap
}

fn first_walk(v: usize, tree: &BuchheimTree, nodes: &mut [Node]) {
    let kids = &tree.children[v];
    if kids.is_empty() {
        nodes[v].prelim = match left_sibling(v, tree, nodes) {
            Some(w) => nodes[w].prelim + distance(w, v, nodes, tree.sibling_gap),
            None => 0.0,
        };
        return;
    }
    let mut default_ancestor = kids[0];
    for &c in kids {
        first_walk(c, tree, nodes);
        default_ancestor = apportion(c, default_ancestor, tree, nodes);
    }
    execute_shifts(v, tree, nodes);
    let first = kids[0];
    let last = *kids.last().unwrap();
    let mid = (nodes[first].prelim + nodes[last].prelim) / 2.0;
    if let Some(w) = left_sibling(v, tree, nodes) {
        nodes[v].prelim = nodes[w].prelim + distance(w, v, nodes, tree.sibling_gap);
        nodes[v].modifier = nodes[v].prelim - mid;
    } else {
        nodes[v].prelim = mid;
    }
}

fn apportion(v: usize, mut default_ancestor: usize, tree: &BuchheimTree, nodes: &mut [Node]) -> usize {
    let Some(w) = left_sibling(v, tree, nodes) else {
        return default_ancestor;
    };
    let mut vir = v;
    let mut vor = v;
    let mut vil = w;
    let mut vol = leftmost_sibling(v, tree, nodes).unwrap_or(v);
    let mut sir = nodes[v].modifier;
    let mut sor = nodes[v].modifier;
    let mut sil = nodes[vil].modifier;
    let mut sol = nodes[vol].modifier;
    while let (Some(next_vil), Some(next_vir)) = (next_right(vil, tree, nodes), next_left(vir, tree, nodes))
    {
        vil = next_vil;
        vir = next_vir;
        vol = next_left(vol, tree, nodes).unwrap_or(vol);
        vor = next_right(vor, tree, nodes).unwrap_or(vor);
        nodes[vor].ancestor = v;
        let shift = (nodes[vil].prelim + sil) - (nodes[vir].prelim + sir)
            + distance(vil, vir, nodes, tree.sibling_gap);
        if shift > 0.0 {
            let a = ancestor(vil, v, default_ancestor, nodes);
            move_subtree(a, v, shift, nodes);
            sir += shift;
            sor += shift;
        }
        sil += nodes[vil].modifier;
        sir += nodes[vir].modifier;
        sol += nodes[vol].modifier;
        sor += nodes[vor].modifier;
    }
    if next_right(vil, tree, nodes).is_some() && next_right(vor, tree, nodes).is_none() {
        nodes[vor].thread = next_right(vil, tree, nodes);
        nodes[vor].modifier += sil - sor;
    }
    if next_left(vir, tree, nodes).is_some() && next_left(vol, tree, nodes).is_none() {
        nodes[vol].thread = next_left(vir, tree, nodes);
        nodes[vol].modifier += sir - sol;
        default_ancestor = v;
    }
    default_ancestor
}

fn move_subtree(wm: usize, wp: usize, shift: f64, nodes: &mut [Node]) {
    let subtrees = (nodes[wp].child_index as f64) - (nodes[wm].child_index as f64);
    if subtrees <= 0.0 {
        nodes[wp].prelim += shift;
        nodes[wp].modifier += shift;
        return;
    }
    nodes[wp].change -= shift / subtrees;
    nodes[wp].shift += shift;
    nodes[wm].change += shift / subtrees;
    nodes[wp].prelim += shift;
    nodes[wp].modifier += shift;
}

fn execute_shifts(v: usize, tree: &BuchheimTree, nodes: &mut [Node]) {
    let mut shift = 0.0;
    let mut change = 0.0;
    for &w in tree.children[v].iter().rev() {
        nodes[w].prelim += shift;
        nodes[w].modifier += shift;
        change += nodes[w].change;
        shift += nodes[w].shift + change;
    }
}

fn ancestor(vil: usize, v: usize, default_ancestor: usize, nodes: &[Node]) -> usize {
    let a = nodes[vil].ancestor;
    if nodes[a].parent == nodes[v].parent && a != v {
        a
    } else {
        default_ancestor
    }
}

fn second_walk(v: usize, tree: &BuchheimTree, nodes: &[Node], m: f64, x: &mut [f64]) {
    x[v] = nodes[v].prelim + m;
    for &c in &tree.children[v] {
        second_walk(c, tree, nodes, m + nodes[v].modifier, x);
    }
}

fn left_sibling(v: usize, tree: &BuchheimTree, nodes: &[Node]) -> Option<usize> {
    let p = nodes[v].parent?;
    let i = nodes[v].child_index;
    if i == 0 {
        None
    } else {
        Some(tree.children[p][i - 1])
    }
}

fn leftmost_sibling(v: usize, tree: &BuchheimTree, nodes: &[Node]) -> Option<usize> {
    let p = nodes[v].parent?;
    tree.children[p].first().copied()
}

fn next_left(v: usize, tree: &BuchheimTree, nodes: &[Node]) -> Option<usize> {
    tree.children[v]
        .first()
        .copied()
        .or(nodes[v].thread)
}

fn next_right(v: usize, tree: &BuchheimTree, nodes: &[Node]) -> Option<usize> {
    tree.children[v]
        .last()
        .copied()
        .or(nodes[v].thread)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(root: usize, children: Vec<Vec<usize>>, widths: Vec<f64>, gap: f64) -> BuchheimTree {
        BuchheimTree {
            root,
            children,
            widths,
            sibling_gap: gap,
        }
    }

    fn almost(a: f64, b: f64) {
        assert!(
            (a - b).abs() < 1e-9,
            "expected {b}, got {a}"
        );
    }

    #[test]
    fn table_driven_centers() {
        struct Case {
            name: &'static str,
            t: BuchheimTree,
            check: fn(&[f64]),
        }
        let cases = [
            Case {
                name: "single",
                t: tree(0, vec![vec![]], vec![10.0], 4.0),
                check: |x| almost(x[0], 0.0),
            },
            Case {
                name: "binary-equal",
                t: tree(
                    0,
                    vec![vec![1, 2], vec![], vec![]],
                    vec![10.0, 10.0, 10.0],
                    4.0,
                ),
                check: |x| {
                    // distance = 5+5+4 = 14; parent at mid of children.
                    almost(x[1], x[0] - 7.0);
                    almost(x[2], x[0] + 7.0);
                    almost(x[2] - x[1], 14.0);
                },
            },
            Case {
                name: "unequal-widths",
                t: tree(
                    0,
                    vec![vec![1, 2], vec![], vec![]],
                    vec![10.0, 20.0, 10.0],
                    4.0,
                ),
                check: |x| {
                    // distance(1,2) = 10+5+4 = 19
                    almost(x[2] - x[1], 19.0);
                    almost(x[0], (x[1] + x[2]) / 2.0);
                },
            },
            Case {
                name: "ternary",
                t: tree(
                    0,
                    vec![vec![1, 2, 3], vec![], vec![], vec![]],
                    vec![10.0, 10.0, 10.0, 10.0],
                    4.0,
                ),
                check: |x| {
                    almost(x[2] - x[1], 14.0);
                    almost(x[3] - x[2], 14.0);
                    almost(x[0], (x[1] + x[3]) / 2.0);
                },
            },
        ];
        for c in cases {
            let x = place(&c.t).unwrap_or_else(|e| panic!("{}: {e}", c.name));
            (c.check)(&x);
            let y = place(&c.t).unwrap();
            assert_eq!(x, y, "{}: not deterministic", c.name);
        }
    }

    #[test]
    fn mirror_reverses_x() {
        let t = tree(
            0,
            vec![vec![1, 2], vec![3, 4], vec![5], vec![], vec![], vec![]],
            vec![10.0; 6],
            4.0,
        );
        let x = place(&t).unwrap();
        let mut children = t.children.clone();
        children[0] = vec![2, 1];
        children[1] = vec![4, 3];
        let tm = BuchheimTree {
            children,
            ..t.clone()
        };
        let xm = place(&tm).unwrap();
        // Mirror about the root: offsets flip sign.
        almost(x[1] - x[0], -(xm[1] - xm[0]));
        almost(x[2] - x[0], -(xm[2] - xm[0]));
        almost(x[3] - x[1], -(xm[3] - xm[1]));
        almost(x[4] - x[1], -(xm[4] - xm[1]));
        almost(x[5] - x[2], -(xm[5] - xm[2]));
    }

    #[test]
    fn isomorphic_subtrees_same_shape() {
        //     0
        //    / \
        //   1   2
        //  / \ / \
        // 3  4 5  6
        let t = tree(
            0,
            vec![
                vec![1, 2],
                vec![3, 4],
                vec![5, 6],
                vec![],
                vec![],
                vec![],
                vec![],
            ],
            vec![10.0; 7],
            4.0,
        );
        let x = place(&t).unwrap();
        almost(x[4] - x[3], x[6] - x[5]);
        almost(x[1] - x[3], x[2] - x[5]);
        almost(x[4] - x[1], x[6] - x[2]);
    }

    #[test]
    fn empty_and_bad_input() {
        assert!(place(&tree(0, vec![], vec![], 4.0)).unwrap().is_empty());
        assert!(place(&tree(0, vec![vec![]], vec![10.0, 1.0], 4.0)).is_err());
        assert!(place(&tree(3, vec![vec![]], vec![10.0], 4.0)).is_err());
        assert!(place(&tree(0, vec![vec![0]], vec![10.0], 4.0)).is_err());
    }
}
