/**
 * 一键示例 Prompts
 *
 * 演示场景下，用户无需思考"该怎么提问"，点击即可发车。
 * 覆盖主流图表类型 + 1 个增量编辑示例（让用户感受 apply_patch 的能力）。
 */

export interface ExamplePrompt {
  /** 一行标题（按钮文字） */
  title: string;
  /** emoji 图标，纯展示 */
  icon: string;
  /** 完整 prompt 文本，点按后直接作为用户消息发送 */
  prompt: string;
  /** 短描述（鼠标 hover 提示） */
  hint?: string;
}

export const EXAMPLE_PROMPTS: ExamplePrompt[] = [
  {
    title: '电商下单流程',
    icon: '🛒',
    prompt:
      '画一个电商下单流程的流程图，包含：用户浏览商品、加入购物车、提交订单、支付、库存校验、发货。决策点要标注失败分支（如支付失败、库存不足）。',
    hint: 'flowchart · 决策分支',
  },
  {
    title: '微服务架构图',
    icon: '🏗️',
    prompt:
      '画一个典型微服务架构图，包含：API 网关、用户服务、订单服务、商品服务、Redis 缓存、PostgreSQL、Kafka 消息队列、Prometheus 监控。用 semantic 匹配图标。',
    hint: 'architecture · semantic 图标',
  },
  {
    title: '用户认证时序图',
    icon: '⏱️',
    prompt:
      '画一个 OAuth2 用户认证的时序图，参与者：用户、浏览器、认证服务、资源服务、Redis。流程：授权码签发、Token 交换、资源访问。',
    hint: 'sequence · 时序交互',
  },
  {
    title: '订单状态机',
    icon: '🔄',
    prompt:
      '画一个订单状态机图：待支付 → 已支付 → 待发货 → 已发货 → 已签收；分支：支付超时取消、签收后退货退款。',
    hint: 'state · 状态流转',
  },
  {
    title: '博客 ER 图',
    icon: '🗃️',
    prompt:
      '画一个博客系统的 ER 图，实体：User、Post、Comment、Tag、Category。关系：User 1对多 Post、Post 1对多 Comment、Post 多对多 Tag。',
    hint: 'er · 实体关系',
  },
  {
    title: 'AI 学习路线',
    icon: '🧠',
    prompt:
      '画一个 AI 学习路线的思维导图，根节点是"AI 学习路线"，分支：数学基础、机器学习、深度学习、工程实践，每个分支再细分 3-4 个子主题。',
    hint: 'mindmap · 思维导图',
  },
];

/** 第二轮示例：让用户感受增量编辑能力 */
export const INCREMENTAL_EXAMPLES: ExamplePrompt[] = [
  {
    title: '给架构图加缓存层',
    icon: '➕',
    prompt: '在上一个架构图基础上，给订单服务和商品服务之间加一层本地缓存，并标注缓存失效策略。',
    hint: 'apply_patch · 增量修改',
  },
  {
    title: '换横向布局',
    icon: '↔️',
    prompt: '把当前图表的布局方向改成从左到右（left-to-right），其他内容保持不变。',
    hint: 'apply_patch · 单属性修改',
  },
];
