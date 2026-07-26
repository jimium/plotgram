//! Atlas 下一代布局与路由架构（22 号文）的孵化子树。
//!
//! **本子树不接线生产**：生产路径仍走 `pipeline` → `recipes` → `routing`。
//! 按 23 号文 D1 决策，Atlas 组件在此逐 Stage 孵化，经影子对拍验证后接管旧路径。
//!
//! 当前包含：
//! - [`channel`]：抽象通道图（相 I 通道规划的无坐标选路原型）
//! - [`plan`]：Plan IR（整图离散决策的唯一中间表示，可序列化/指纹/diff）
//! - [`probe`]：可行率探针门面（Diagram → ChannelBlueprint，桥接私有分层内核）
//! - [`provenance`]：几何量溯源（Stage 0 交付 0.1）
//! - [`space`]：`Occupant` / `Demand` 空间占用抽象（Stage 0 交付 0.2）
//! - [`pipeline`]：`AtlasPipeline` 入口壳，Stage 0 转发 legacy（交付 0.3）
//! - [`shadow`]：新旧管线影子对拍器 + `ShadowReport`（交付 0.5）

pub mod channel;
pub mod pipeline;
pub mod plan;
pub mod probe;
pub mod provenance;
pub mod shadow;
pub mod space;
