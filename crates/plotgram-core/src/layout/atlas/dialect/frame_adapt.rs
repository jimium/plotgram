//! `group_frame` DSL → HierarchicalProfile 适配器（23 §7.5.7 / doc 21 §5.2）。
//!
//! 迁移期：旧 `group_frame:` / 顶层 `group_sizing` / `group_align` / `density`
//! 属性覆盖 Scheme 默认 Profile。终态 DSL 应直接写 Profile 字段；本适配器可删。

use crate::ast::{AttributeValue, Diagram};
use crate::types::standard_attr_keys::diagram as dsl;

use super::profile::{Density, GroupAlign, GroupSizing, HierarchicalProfile};

/// 将 diagram 上的遗留布局 attr 叠到 Profile（就地修改）。
pub fn apply_legacy_layout_attrs(diagram: &Diagram, profile: &mut HierarchicalProfile) {
    apply_group_frame(diagram, profile);
    apply_flat_profile_attrs(diagram, profile);
}

fn apply_flat_profile_attrs(diagram: &Diagram, profile: &mut HierarchicalProfile) {
    for attr in &diagram.attributes {
        match attr.key.as_str() {
            "group_sizing" => {
                if let Some(s) = attr.value.as_str() {
                    match s {
                        "equal" | "uniform" | "equal_siblings" => {
                            profile.group_sizing = GroupSizing::Equal;
                        }
                        "fit" => profile.group_sizing = GroupSizing::Fit,
                        _ => {}
                    }
                }
            }
            "group_align" => {
                if let Some(s) = attr.value.as_str() {
                    match s {
                        "start" | "left" => profile.group_align = GroupAlign::Start,
                        "center" => profile.group_align = GroupAlign::Center,
                        "end" | "right" => profile.group_align = GroupAlign::End,
                        _ => {}
                    }
                }
            }
            "density" => {
                if let Some(s) = attr.value.as_str() {
                    match s {
                        "compact" => profile.density = Density::Compact,
                        "standard" => profile.density = Density::Standard,
                        "spacious" => profile.density = Density::Spacious,
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
}

fn apply_group_frame(diagram: &Diagram, profile: &mut HierarchicalProfile) {
    let Some(attr) = diagram.attributes.iter().find(|a| a.key == dsl::GROUP_FRAME) else {
        return;
    };
    let options = match &attr.value {
        AttributeValue::Config { options, .. } => options,
        AttributeValue::String(s) => {
            // `group_frame: strips` → Equal；`stack` / 其它 → 保持
            if s.as_str() == "strips" {
                profile.group_sizing = GroupSizing::Equal;
            }
            return;
        }
        _ => return,
    };

    if let Some(v) = options.get("track").and_then(|v| v.as_str()) {
        match v {
            "equal" | "uniform" => profile.group_sizing = GroupSizing::Equal,
            "fit" => profile.group_sizing = GroupSizing::Fit,
            _ => {}
        }
    }
    if let Some(v) = options.get("align").or_else(|| options.get("cross")).and_then(|v| v.as_str())
    {
        match v {
            "start" | "left" => profile.group_align = GroupAlign::Start,
            "center" => profile.group_align = GroupAlign::Center,
            "end" | "right" => profile.group_align = GroupAlign::End,
            _ => {}
        }
    }
    if let Some(AttributeValue::Number(g)) = options.get("gap") {
        // 粗映射到 density 档
        if *g < 36.0 {
            profile.density = Density::Compact;
        } else if *g > 64.0 {
            profile.density = Density::Spacious;
        } else {
            profile.density = Density::Standard;
        }
    }
}
