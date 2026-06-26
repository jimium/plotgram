//! 属性枚举值常量（value-side）。
//!
//! 与 [`super::standard_attr_keys`]（key-side）对称：
//! - `standard_attr_keys`：属性键名常量（`TYPE`、`STATUS`、`DIRECTION`…）
//! - `attr_constants`：属性值的枚举常量（`entity_type::SERVICE`、`status::HEALTHY`…）
//!
//! 消费方引用常量而非魔法字符串，集中管理避免 drift。

/// 实体类型枚举值（各 diagram type 的并集）。
///
/// 注意：具体图表类型允许的子集由 `DiagramProfile::entity_types` 收窄，
/// 此处仅提供全量常量供引用。
pub mod entity_type {
    pub const CLIENT: &str = "client";
    pub const QUEUE: &str = "queue";
    pub const CACHE: &str = "cache";
    pub const GATEWAY: &str = "gateway";
    pub const STORAGE: &str = "storage";
    pub const EXTERNAL: &str = "external";
    pub const DECISION: &str = "decision";
    pub const PROCESS: &str = "process";
    pub const START: &str = "start";
    pub const END: &str = "end";
    pub const PARTICIPANT: &str = "participant";
    pub const ACTOR: &str = "actor";
    pub const BOUNDARY: &str = "boundary";
    pub const CONTROL: &str = "control";
    pub const ENTITY: &str = "entity";
    pub const DATABASE: &str = "database";
    pub const FRONTEND: &str = "frontend";
    pub const BACKEND: &str = "backend";
    pub const SERVICE: &str = "service";
    pub const PERSON: &str = "person";
    pub const INITIAL: &str = "initial";
    pub const STATE: &str = "state";
    pub const FINAL: &str = "final";
    pub const CHOICE: &str = "choice";
    pub const ROOT: &str = "root";
    pub const MAIN: &str = "main";
    pub const BRANCH: &str = "branch";
    pub const LEAF: &str = "leaf";

    pub const ALL: &[&str] = &[
        CLIENT, QUEUE, CACHE, GATEWAY, STORAGE, EXTERNAL, DECISION, PROCESS, START, END,
        PARTICIPANT, ACTOR, BOUNDARY, CONTROL, ENTITY, DATABASE, FRONTEND, BACKEND, SERVICE,
        PERSON, INITIAL, STATE, FINAL, CHOICE, ROOT, MAIN, BRANCH, LEAF,
    ];
}

/// 状态枚举值（entity / relation 共用）。
pub mod status {
    pub const HEALTHY: &str = "healthy";
    pub const DEGRADED: &str = "degraded";
    pub const DOWN: &str = "down";
    pub const UNKNOWN: &str = "unknown";

    pub const ALL: &[&str] = &[HEALTHY, DEGRADED, DOWN, UNKNOWN];
}

/// 布局方向枚举值（diagram 级 `direction` 属性）。
pub mod direction {
    pub const TOP_TO_BOTTOM: &str = "top-to-bottom";
    pub const LEFT_TO_RIGHT: &str = "left-to-right";
    pub const RADIAL: &str = "radial";

    pub const ALL: &[&str] = &[TOP_TO_BOTTOM, LEFT_TO_RIGHT, RADIAL];
}

/// 分组边框样式枚举值（group 级 `border_style` 属性）。
pub mod group_border_style {
    pub const SOLID: &str = "solid";
    pub const DASHED: &str = "dashed";
    pub const DOTTED: &str = "dotted";

    pub const ALL: &[&str] = &[SOLID, DASHED, DOTTED];
}

/// 分组内布局枚举值（group 级 `layout` 属性）。
pub mod group_layout {
    pub const AUTO: &str = "auto";
    pub const HORIZONTAL: &str = "horizontal";
    pub const VERTICAL: &str = "vertical";
    pub const FAN_OUT: &str = "fan-out";
    pub const FAN_IN: &str = "fan-in";
    pub const GRID: &str = "grid";

    pub const ALL: &[&str] = &[AUTO, HORIZONTAL, VERTICAL, FAN_OUT, FAN_IN, GRID];
}

/// 分组尺寸策略枚举值（diagram 级 `group_sizing` 属性）。
pub mod group_sizing {
    pub const FIT: &str = "fit";
    pub const UNIFORM: &str = "uniform";

    pub const ALL: &[&str] = &[FIT, UNIFORM];
}

/// 分组对齐方式枚举值（diagram 级 `group_align` 属性）。
pub mod group_align {
    pub const CENTER: &str = "center";
    pub const LEFT: &str = "left";

    pub const ALL: &[&str] = &[CENTER, LEFT];
}

/// 分组排列方向枚举值（diagram 级 `group_arrangement` 属性）。
pub mod group_arrangement {
    pub const VERTICAL: &str = "vertical";
    pub const HORIZONTAL: &str = "horizontal";

    pub const ALL: &[&str] = &[VERTICAL, HORIZONTAL];
}
