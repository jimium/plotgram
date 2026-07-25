//! 分组层级索引（布局 EGB 与边路由共享）。

use std::collections::HashMap;

use crate::ast::Diagram;
use crate::layout::GroupLayout;

use super::constants::EPS;

const GROUP_GAP_THRESHOLD: f64 = 48.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiblingOrientation {
    Horizontal,
    Vertical,
}

/// 分组层级元数据（由 diagram + group 几何确定性构建）。
#[derive(Debug, Clone)]
pub struct GroupHierarchy {
    pub node_leaf_group: HashMap<String, String>,
    pub parent_of: HashMap<String, String>,
    pub group_ancestors: HashMap<String, Vec<String>>,
    pub sibling_sets: Vec<Vec<String>>,
    pub sibling_orientation: HashMap<(String, String), SiblingOrientation>,
}

pub fn build_group_hierarchy(
    diagram: &Diagram,
    groups: &HashMap<String, GroupLayout>,
) -> GroupHierarchy {
    let mut parent_of: HashMap<String, String> = HashMap::new();
    let mut children_of: HashMap<String, Vec<String>> = HashMap::new();
    let mut group_depth: HashMap<String, u8> = HashMap::new();

    for group in &diagram.groups {
        let gid = group.id.as_str().to_string();
        group_depth.insert(gid.clone(), group.depth);
        if let Some(pid) = &group.parent_id {
            let pid_str = pid.as_str().to_string();
            parent_of.insert(gid.clone(), pid_str.clone());
            children_of.entry(pid_str).or_default().push(gid);
        }
    }

    for children in children_of.values_mut() {
        children.sort();
    }

    let mut sibling_sets: Vec<Vec<String>> = Vec::new();
    for children in children_of.values() {
        if children.len() >= 2 {
            sibling_sets.push(children.clone());
        }
    }
    sibling_sets.sort();

    let mut sibling_orientation: HashMap<(String, String), SiblingOrientation> = HashMap::new();
    for siblings in &sibling_sets {
        for i in 0..siblings.len() {
            for j in (i + 1)..siblings.len() {
                let ga = &siblings[i];
                let gb = &siblings[j];
                if let (Some(gla), Some(glb)) = (groups.get(ga), groups.get(gb)) {
                    let ox = range_overlap(gla.x, gla.x + gla.width, glb.x, glb.x + glb.width);
                    let oy = range_overlap(gla.y, gla.y + gla.height, glb.y, glb.y + glb.height);
                    let min_h = gla.height.min(glb.height);
                    let min_w = gla.width.min(glb.width);

                    let orient = if oy >= 0.5 * min_h - EPS && ox <= GROUP_GAP_THRESHOLD {
                        SiblingOrientation::Horizontal
                    } else if ox >= 0.5 * min_w - EPS && oy <= GROUP_GAP_THRESHOLD {
                        SiblingOrientation::Vertical
                    } else {
                        let dx = (gla.x + gla.width / 2.0) - (glb.x + glb.width / 2.0);
                        let dy = (gla.y + gla.height / 2.0) - (glb.y + glb.height / 2.0);
                        if dy.abs() >= dx.abs() {
                            SiblingOrientation::Vertical
                        } else {
                            SiblingOrientation::Horizontal
                        }
                    };
                    let key = if ga <= gb {
                        (ga.clone(), gb.clone())
                    } else {
                        (gb.clone(), ga.clone())
                    };
                    sibling_orientation.insert(key, orient);
                }
            }
        }
    }

    let mut group_ancestors: HashMap<String, Vec<String>> = HashMap::new();
    for group in &diagram.groups {
        let gid = group.id.as_str().to_string();
        let mut ancestors = Vec::new();
        let mut current = parent_of.get(&gid).cloned();
        while let Some(p) = current {
            ancestors.push(p.clone());
            current = parent_of.get(&p).cloned();
        }
        group_ancestors.insert(gid, ancestors);
    }

    let mut node_leaf_group: HashMap<String, String> = HashMap::new();
    for group in &diagram.groups {
        let gid = group.id.as_str().to_string();
        for eid in &group.entity_ids {
            let eid_str = eid.as_str().to_string();
            let existing_depth = node_leaf_group
                .get(&eid_str)
                .and_then(|g| group_depth.get(g))
                .copied()
                .unwrap_or(0);
            if group.depth >= existing_depth {
                node_leaf_group.insert(eid_str, gid.clone());
            }
        }
    }

    GroupHierarchy {
        node_leaf_group,
        parent_of,
        group_ancestors,
        sibling_sets,
        sibling_orientation,
    }
}

/// 两 leaf group 的最低公共祖先（含链上节点）；无公共祖先时 `None`。
pub fn lowest_common_ancestor(
    a: &str,
    b: &str,
    parent_of: &HashMap<String, String>,
) -> Option<String> {
    let mut ancestors_b: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut cur = Some(b.to_string());
    while let Some(g) = cur {
        ancestors_b.insert(g.clone());
        cur = parent_of.get(&g).cloned();
    }
    cur = Some(a.to_string());
    while let Some(g) = cur {
        if ancestors_b.contains(&g) {
            return Some(g);
        }
        cur = parent_of.get(&g).cloned();
    }
    None
}

/// 从 `leaf` 沿父链向上直到 `stop`（均含）的 group id，顺序为 `stop → … → leaf`。
pub fn path_from_leaf_up_to_including(
    leaf: &str,
    stop: &str,
    parent_of: &HashMap<String, String>,
) -> Vec<String> {
    let mut path_rev = Vec::new();
    let mut cur = Some(leaf.to_string());
    while let Some(g) = cur {
        path_rev.push(g.clone());
        if g == stop {
            break;
        }
        cur = parent_of.get(&g).cloned();
    }
    path_rev.reverse();
    path_rev
}

/// `g` 的祖先集合（不含 `g` 自身），与 `group_ancestors` 一致。
pub fn ancestor_set_excluding_self(
    g: &str,
    group_ancestors: &HashMap<String, Vec<String>>,
) -> std::collections::HashSet<String> {
    group_ancestors
        .get(g)
        .into_iter()
        .flatten()
        .cloned()
        .collect()
}

fn range_overlap(a_min: f64, a_max: f64, b_min: f64, b_max: f64) -> f64 {
    (a_max.min(b_max) - a_min.max(b_min)).max(0.0)
}
