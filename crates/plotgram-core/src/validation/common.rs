use std::collections::HashSet;

use crate::types::standard_attr_keys::{diagram, entity, group};
use crate::types::attr_constants;
use crate::types::attr_schema;
use crate::ast::{is_valid_atom, AttributeValue, Diagram};
use crate::error::{DiagnosticError, ValidationResult};
use crate::layout::recipes::architecture::{is_valid_group_layout_atom, VALID_GROUP_LAYOUTS};

use crate::types::style_attrs::{is_atom_like, is_boolean_like, is_number_like, is_string_like};

fn is_valid_option_value(value: &AttributeValue) -> bool {
    is_string_like(value)
        || is_number_like(value)
        || is_boolean_like(value)
        || is_atom_like(value)
}

pub fn validate_diagram_attributes(diagram: &Diagram, result: &mut ValidationResult) {
    let mut seen_keys = HashSet::new();

    for attr in &diagram.attributes {
        if !seen_keys.insert(&attr.key) {
            result.add_error(DiagnosticError::structure_violation(
                attr.span,
                format!("diagram 属性 '{}' 重复声明", attr.key),
            ));
            continue;
        }

        match attr.key.as_str() {
            diagram::DIRECTION
            | diagram::LAYOUT
            | diagram::EDGE_ROUTING
            | diagram::GROUP_FRAME
            | diagram::THEME
            | diagram::RENDER_STYLE
            | diagram::PIPELINE => match &attr.value {
                AttributeValue::String(_) => {
                    if !is_atom_like(&attr.value) {
                        result.add_error(DiagnosticError::structure_violation(
                            attr.span,
                            format!("属性 '{}' 的值必须是 atom", attr.key),
                        ));
                    }
                    if let Some(v) = attr.value.as_str() {
                        if let Some(valid_values) = attr_schema::enum_values_for_key(&attr.key) {
                            if !valid_values.contains(&v) {
                                result.add_error(DiagnosticError::invalid_enum_value(
                                    attr.span,
                                    &attr.key,
                                    v,
                                    valid_values,
                                ));
                            }
                        }
                    }
                }
                AttributeValue::Config { algo, options } => {
                    if !is_valid_atom(algo) {
                        result.add_error(DiagnosticError::structure_violation(
                            attr.span,
                            format!("算法名 '{algo}' 不是合法的 atom"),
                        ));
                    }
                    for (opt_key, opt_value) in options {
                        if !is_valid_option_value(opt_value) {
                            result.add_error(DiagnosticError::structure_violation(
                                attr.span,
                                format!(
                                    "属性 '{}' 的选项 '{opt_key}' 必须是 string、number、boolean 或 atom",
                                    attr.key
                                ),
                            ));
                        }
                    }
                }
                _ => {
                    result.add_error(DiagnosticError::structure_violation(
                        attr.span,
                        format!("属性 '{}' 的值必须是 atom 或算法配置块", attr.key),
                    ));
                }
            },
            diagram::SNAP => {
                if !matches!(attr.value, AttributeValue::Boolean(_)) {
                    result.add_error(DiagnosticError::structure_violation(
                        attr.span,
                        format!("属性 '{}' 的值必须是 boolean（true 或 false）", diagram::SNAP),
                    ));
                }
            }
            diagram::ALIGN => match &attr.value {
                AttributeValue::Boolean(_) => {}
                AttributeValue::String(tv) => {
                    let v = tv.as_str();
                    if !attr_constants::align::ALL_ATOMS.contains(&v) {
                        result.add_error(DiagnosticError::invalid_enum_value(
                            attr.span,
                            diagram::ALIGN,
                            v,
                            attr_constants::align::ALL_ATOMS,
                        ));
                    }
                }
                _ => {
                    result.add_error(DiagnosticError::structure_violation(
                        attr.span,
                        format!(
                            "属性 '{}' 的值必须是 boolean（true/false）或 atom（rank/layer/full/off）",
                            diagram::ALIGN
                        ),
                    ));
                }
            }
            diagram::TITLE => {
                if !is_string_like(&attr.value) {
                    result.add_error(DiagnosticError::structure_violation(
                        attr.span,
                        format!("属性 '{}' 的值必须是 string", diagram::TITLE),
                    ));
                }
            }
            key if diagram::removed::ALL.contains(&key) => {
                result.add_error(DiagnosticError::structure_violation(
                    attr.span,
                    format!(
                        "属性 '{key}' 已移除，请改用 group_frame: stack {{ ... }}（例如 track/axis/gap/cross）"
                    ),
                ));
            }
            _ => {
                result.add_error(DiagnosticError::structure_violation(
                    attr.span,
                    format!("未知的 diagram 属性 '{}'", attr.key),
                ));
            }
        }
    }
}

pub fn validate_relations(diagram: &Diagram, result: &mut ValidationResult) {
    let entity_ids: HashSet<&str> = diagram.entities.iter().map(|e| e.id.as_str()).collect();
    let group_ids: HashSet<&str> = diagram.groups.iter().map(|g| g.id.as_str()).collect();
    let all_ids: HashSet<&str> = entity_ids.union(&group_ids).copied().collect();
    let available: Vec<String> = diagram
        .entities
        .iter()
        .map(|e| e.id.as_str().to_string())
        .collect();

    for relation in &diagram.relations {
        let from_str = relation.from.as_str();
        let to_str = relation.to.as_str();

        if !all_ids.contains(from_str) {
            result.add_error(DiagnosticError::undefined_reference(
                relation.span,
                from_str,
                &available,
            ));
        } else if group_ids.contains(from_str) {
            result.add_error(DiagnosticError::group_relation(relation.span, from_str));
        }

        if !all_ids.contains(to_str) {
            result.add_error(DiagnosticError::undefined_reference(
                relation.span,
                to_str,
                &available,
            ));
        } else if group_ids.contains(to_str) {
            result.add_error(DiagnosticError::group_relation(relation.span, to_str));
        }
    }
}

/// 校验 DSL `constrain A -> B`：端点、自环、不可满足环，以及布局路径限制。
pub fn validate_constraints(diagram: &Diagram, result: &mut ValidationResult) {
    if diagram.constraints.is_empty() {
        return;
    }

    let entity_ids: HashSet<&str> = diagram.entities.iter().map(|e| e.id.as_str()).collect();
    let group_ids: HashSet<&str> = diagram.groups.iter().map(|g| g.id.as_str()).collect();
    let all_ids: HashSet<&str> = entity_ids.union(&group_ids).copied().collect();
    let available: Vec<String> = diagram
        .entities
        .iter()
        .map(|e| e.id.as_str().to_string())
        .collect();

    let mut endpoint_ok = true;
    for c in &diagram.constraints {
        let from_str = c.from.as_str();
        let to_str = c.to.as_str();

        if !all_ids.contains(from_str) {
            result.add_error(DiagnosticError::undefined_reference(
                c.span,
                from_str,
                &available,
            ));
            endpoint_ok = false;
        } else if group_ids.contains(from_str) {
            result.add_error(DiagnosticError::structure_violation(
                c.span,
                format!("不允许 group '{}' 作为 constrain 端点", from_str),
            ));
            endpoint_ok = false;
        }

        if !all_ids.contains(to_str) {
            result.add_error(DiagnosticError::undefined_reference(
                c.span,
                to_str,
                &available,
            ));
            endpoint_ok = false;
        } else if group_ids.contains(to_str) {
            result.add_error(DiagnosticError::structure_violation(
                c.span,
                format!("不允许 group '{}' 作为 constrain 端点", to_str),
            ));
            endpoint_ok = false;
        }

        if from_str == to_str {
            result.add_error(DiagnosticError::structure_violation(
                c.span,
                format!("constrain 不允许自环：'{}' -> '{}'", from_str, to_str),
            ));
            endpoint_ok = false;
        }
    }

    if endpoint_ok {
        validate_constraint_satisfiability(diagram, result);
    }
}

/// 关系边可反转破环；约束边不可反转。
/// 对关系图做 greedy FAS 得到 DAG 后加入约束边，若仍有环则不可满足。
fn validate_constraint_satisfiability(diagram: &Diagram, result: &mut ValidationResult) {
    use crate::layout::kernel::common::acyclic::greedy_fas;
    use std::collections::{HashMap, HashSet as StdHashSet};

    let mut nodes: StdHashSet<String> = StdHashSet::new();
    for e in &diagram.entities {
        nodes.insert(e.id.as_str().to_string());
    }
    for r in &diagram.relations {
        nodes.insert(r.from.as_str().to_string());
        nodes.insert(r.to.as_str().to_string());
    }
    for c in &diagram.constraints {
        nodes.insert(c.from.as_str().to_string());
        nodes.insert(c.to.as_str().to_string());
    }
    let mut node_list: Vec<String> = nodes.into_iter().collect();
    node_list.sort();

    let mut out_neighbors: HashMap<String, Vec<String>> = HashMap::new();
    let mut in_neighbors: HashMap<String, Vec<String>> = HashMap::new();
    for n in &node_list {
        out_neighbors.insert(n.clone(), Vec::new());
        in_neighbors.insert(n.clone(), Vec::new());
    }
    for r in &diagram.relations {
        let from = r.from.as_str().to_string();
        let to = r.to.as_str().to_string();
        out_neighbors
            .entry(from.clone())
            .or_default()
            .push(to.clone());
        in_neighbors.entry(to).or_default().push(from);
    }

    let reversed = greedy_fas(&node_list, &out_neighbors, &in_neighbors);

    // 构建 FAS 后的 DAG 邻接表（反转边方向）
    let mut dag_out: HashMap<String, Vec<String>> = HashMap::new();
    for n in &node_list {
        dag_out.insert(n.clone(), Vec::new());
    }
    for r in &diagram.relations {
        let from = r.from.as_str();
        let to = r.to.as_str();
        let key = (from.to_string(), to.to_string());
        if reversed.contains(&key) {
            dag_out
                .entry(to.to_string())
                .or_default()
                .push(from.to_string());
        } else {
            dag_out
                .entry(from.to_string())
                .or_default()
                .push(to.to_string());
        }
    }

    // 加入约束边
    for c in &diagram.constraints {
        dag_out
            .entry(c.from.as_str().to_string())
            .or_default()
            .push(c.to.as_str().to_string());
    }

    // DFS 找环；若环上有约束边则报错
    if let Some(span) = find_constraint_in_cycle(&dag_out, &diagram.constraints) {
        result.add_error(DiagnosticError::structure_violation(
            span,
            "constrain 与关系边形成不可满足的环（约束边不可反转）",
        ));
    }
}

/// 在图中检测环；若找到环，返回环上某条约束边的 span。
fn find_constraint_in_cycle(
    out: &std::collections::HashMap<String, Vec<String>>,
    constraints: &[crate::ast::Constraint],
) -> Option<crate::ast::Span> {
    use std::collections::HashSet;

    let constraint_edges: HashSet<(&str, &str)> = constraints
        .iter()
        .map(|c| (c.from.as_str(), c.to.as_str()))
        .collect();

    #[derive(Clone, Copy, PartialEq)]
    enum Color {
        White,
        Gray,
        Black,
    }

    let mut color: std::collections::HashMap<String, Color> = out
        .keys()
        .map(|k| (k.clone(), Color::White))
        .collect();
    // 也覆盖邻接表中作为目标出现的节点
    for succs in out.values() {
        for s in succs {
            color.entry(s.clone()).or_insert(Color::White);
        }
    }

    let mut parent: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    let mut cycle_edge: Option<(String, String)> = None;

    fn dfs(
        u: &str,
        out: &std::collections::HashMap<String, Vec<String>>,
        color: &mut std::collections::HashMap<String, Color>,
        parent: &mut std::collections::HashMap<String, String>,
        cycle_edge: &mut Option<(String, String)>,
    ) -> bool {
        color.insert(u.to_string(), Color::Gray);
        if let Some(succs) = out.get(u) {
            for v in succs {
                match color.get(v).copied().unwrap_or(Color::White) {
                    Color::Gray => {
                        *cycle_edge = Some((u.to_string(), v.clone()));
                        return true;
                    }
                    Color::White => {
                        parent.insert(v.clone(), u.to_string());
                        if dfs(v, out, color, parent, cycle_edge) {
                            return true;
                        }
                    }
                    Color::Black => {}
                }
            }
        }
        color.insert(u.to_string(), Color::Black);
        false
    }

    let mut nodes: Vec<String> = color.keys().cloned().collect();
    nodes.sort();
    for n in &nodes {
        if color.get(n).copied() == Some(Color::White)
            && dfs(n, out, &mut color, &mut parent, &mut cycle_edge)
        {
            break;
        }
    }

    let (back_from, back_to) = cycle_edge?;

    // 收集环上的边，优先报告约束边
    let mut cycle_edges: Vec<(String, String)> = vec![(back_from.clone(), back_to.clone())];
    let mut cur = back_from.clone();
    while cur != back_to {
        let p = parent.get(&cur)?;
        cycle_edges.push((p.clone(), cur.clone()));
        cur = p.clone();
    }

    for (f, t) in &cycle_edges {
        if constraint_edges.contains(&(f.as_str(), t.as_str())) {
            if let Some(c) = constraints
                .iter()
                .find(|c| c.from.as_str() == f && c.to.as_str() == t)
            {
                return Some(c.span);
            }
        }
    }

    // 环存在但回溯未命中约束边（例如约束边就是 back edge）——仍报第一条约束
    constraints.first().map(|c| c.span)
}

pub fn validate_groups(diagram: &Diagram, result: &mut ValidationResult) {
    for group in &diagram.groups {
        for (key, value) in &group.attributes.standard {
            let span = group.attributes.standard_span_or(key, group.span);
            match key.as_str() {
                group::BORDER_STYLE => {
                    if let Some(v) = value.as_str() {
                        if !attr_constants::group_border_style::ALL.contains(&v) {
                            result.add_error(DiagnosticError::invalid_enum_value(
                                span,
                                group::BORDER_STYLE,
                                v,
                                attr_constants::group_border_style::ALL,
                            ));
                        }
                    } else if !is_atom_like(value) {
                        result.add_error(DiagnosticError::structure_violation(
                            span,
                            format!("group '{}' 的 style 属性必须是 atom", group.id),
                        ));
                    }
                }
                group::COLOR => {
                    if !matches!(value, AttributeValue::String(_)) {
                        result.add_error(DiagnosticError::structure_violation(
                            span,
                            format!("group '{}' 的 color 属性必须是字符串", group.id),
                        ));
                    }
                }
                group::LAYOUT => {
                    if let Some(v) = value.as_str() {
                        if !is_valid_group_layout_atom(v) {
                            result.add_error(DiagnosticError::invalid_enum_value(
                                span,
                                group::LAYOUT,
                                v,
                                VALID_GROUP_LAYOUTS,
                            ));
                        }
                    } else if !is_atom_like(value) {
                        result.add_error(DiagnosticError::structure_violation(
                            span,
                            format!("group '{}' 的 layout 属性必须是 atom", group.id),
                        ));
                    }
                }
                _ => {
                    result.add_error(DiagnosticError::structure_violation(
                        span,
                        format!("group '{}' 的未知属性 '{}'", group.id, key),
                    ));
                }
            }
        }

        if group.entity_ids.is_empty() && group.child_group_ids.is_empty() {
            result.add_warning(DiagnosticError::unused_group(group.span, group.id.as_str()));
        }
    }
}

pub fn check_orphan_entities(diagram: &Diagram, result: &mut ValidationResult) {
    let connected: HashSet<&str> = diagram
        .relations
        .iter()
        .flat_map(|r| [r.from.as_str(), r.to.as_str()])
        .collect();

    for entity in &diagram.entities {
        if !connected.contains(entity.id.as_str()) {
            result.add_warning(DiagnosticError::orphan_entity(
                entity.span,
                entity.id.as_str(),
                &entity.label,
            ));
        }
    }
}

/// 校验某规范类型在图表中最多出现 `max_count` 次，超出时对第 `max_count+1` 个及之后的实体报错。
///
/// 泛化了 state（initial 最多 1 个）和 mindmap（root 最多 1 个）的唯一性规则。
/// 错误 span 指向违规实体本身，而非 `entities.first()`。
pub fn validate_unique_canonical_type(
    diagram: &Diagram,
    canonical_type: &str,
    max_count: usize,
    error_message: &str,
    result: &mut ValidationResult,
) {
    let mut seen_count = 0;
    for entity in &diagram.entities {
        let is_match = entity
            .attributes
            .standard
            .get(entity::TYPE)
            .and_then(|v| v.as_str())
            .is_some_and(|raw| raw == canonical_type);
        if is_match {
            seen_count += 1;
            if seen_count > max_count {
                result.add_error(DiagnosticError::structure_violation(
                    entity.span,
                    error_message,
                ));
            }
        }
    }
}

/// 校验自环关系。
///
/// - `exempt_types` 中的实体类型允许自环，但发出 W003 警告（如 flowchart 的 decision）。
/// - 不在 `exempt_types` 中的自环为 E013 错误。
pub fn validate_self_loop(
    diagram: &Diagram,
    exempt_types: &[&str],
    _warning_message: &str,
    result: &mut ValidationResult,
) {
    let entity_type_map: std::collections::HashMap<&str, &str> = diagram
        .entities
        .iter()
        .filter_map(|e| {
            e.attributes
                .standard
                .get(entity::TYPE)
                .and_then(|v| v.as_str())
                .map(|t| (e.id.as_str(), t))
        })
        .collect();

    for relation in &diagram.relations {
        if relation.from == relation.to {
            let entity_type = entity_type_map.get(relation.from.as_str()).copied();
            if exempt_types.contains(&entity_type.unwrap_or("")) {
                // 豁免类型（如 decision）：允许自环，但发出 W003 警告
                result.add_warning(DiagnosticError::self_loop_warning(
                    relation.span,
                    relation.from.as_str(),
                ));
            } else {
                // 非豁免类型：E013 错误
                result.add_error(DiagnosticError::self_loop_error(
                    relation.span,
                    relation.from.as_str(),
                ));
            }
        }
    }
}
