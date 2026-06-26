//! 图类型级分组路由默认参数。

use super::constants::{GROUP_BORDER_SHELL_PAD, PORT_STUB_CLEARANCE};

/// 边路由阶段的分组障碍与走廊软约束参数。
#[derive(Debug, Clone, Copy)]
pub struct GroupRoutingProfile {
    pub border_shell_pad: f64,
    pub stub_clearance: f64,
    pub corridor_misalignment_penalty: f64,
    pub repulse_max_rounds: usize,
}

/// 从 diagram 类型推断布局算法名（用于路由阶段 profile 选择）。
pub fn routing_algo_for_diagram(diagram: &crate::ast::Diagram) -> &'static str {
    use crate::types::DiagramType;
    match diagram.diagram_type {
        DiagramType::Architecture => "architecture",
        DiagramType::Flowchart => "flowchart",
        _ => "sugiyama-v2",
    }
}

impl GroupRoutingProfile {
    pub fn for_algo(algo: &str) -> Self {
        match algo {
            "architecture" => Self::architecture(),
            "flowchart" => Self::flowchart(),
            _ => Self::default_sugiyama(),
        }
    }

    pub fn for_diagram(diagram: &crate::ast::Diagram) -> Self {
        Self::for_algo(routing_algo_for_diagram(diagram))
    }

    pub fn architecture() -> Self {
        Self {
            border_shell_pad: GROUP_BORDER_SHELL_PAD,
            stub_clearance: PORT_STUB_CLEARANCE,
            corridor_misalignment_penalty: 120.0,
            repulse_max_rounds: 2,
        }
    }

    pub fn flowchart() -> Self {
        Self {
            border_shell_pad: GROUP_BORDER_SHELL_PAD,
            stub_clearance: PORT_STUB_CLEARANCE,
            corridor_misalignment_penalty: 80.0,
            repulse_max_rounds: 2,
        }
    }

    pub fn default_sugiyama() -> Self {
        Self::architecture()
    }
}
