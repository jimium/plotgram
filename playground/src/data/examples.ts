import userAuth from '../../../showcase/flowchart/product.user-auth.pgm?raw';
import orderApproval from '../../../showcase/flowchart/product.order-approval.pgm?raw';
import employeeOnboarding from '../../../showcase/flowchart/product.employee-onboarding.pgm?raw';
import oauthLogin from '../../../showcase/sequence/product.oauth-login.pgm?raw';
import paymentGateway from '../../../showcase/sequence/product.payment-gateway.pgm?raw';
import microservices from '../../../showcase/architecture/product.microservices.pgm?raw';
import dataPipeline from '../../../showcase/architecture/product.data-pipeline.pgm?raw';
import ecommercePlatform from '../../../showcase/architecture/product.ecommerce-platform.pgm?raw';
import threeTier from '../../../showcase/architecture/product.three-tier.pgm?raw';
import aiAgentDocops from '../../../showcase/architecture/demo.ai-agent-docops-pipeline.pgm?raw';
import orderLifecycle from '../../../showcase/state/product.order-lifecycle.pgm?raw';
import k8sRolloutState from '../../../showcase/state/demo.k8s-rollout-state-machine.pgm?raw';
import onOffState from '../../../showcase/state/smoke.on-off.pgm?raw';
import blogSchema from '../../../showcase/er/product.blog-schema.pgm?raw';
import socialNetwork from '../../../showcase/er/product.social-network.pgm?raw';
import techStack from '../../../showcase/mindmap/product.tech-stack.pgm?raw';
import programmingLearning from '../../../showcase/mindmap/product.programming-learning.pgm?raw';
import aiLearningRoadmap from '../../../showcase/mindmap/demo.ai-learning-roadmap.pgm?raw';

import stressDag from '../../../showcase/flowchart/stress.layout-stress-dag.pgm?raw';
import stressNested from '../../../showcase/architecture/stress.layout-stress-nested.pgm?raw';
import stressDense from '../../../showcase/er/stress.layout-stress-dense.pgm?raw';
import stressTransitions from '../../../showcase/state/stress.layout-stress-transitions.pgm?raw';
import stressLifelines from '../../../showcase/sequence/stress.layout-stress-lifelines.pgm?raw';

export type DiagramKind =
  | 'flowchart'
  | 'sequence'
  | 'architecture'
  | 'state'
  | 'er'
  | 'mindmap';

export type ExampleCategory = 'basic' | 'scenario' | 'stress';

export interface Example {
  id: string;
  title: string;
  kind: DiagramKind;
  category: ExampleCategory;
  description: string;
  source: string;
  featured?: boolean;
}

export const EXAMPLES: Example[] = [
  // ── 精选示例 ─────────────────────────────────────────────
  {
    id: 'state-k8s-rollout',
    title: 'K8s 发布状态机',
    kind: 'state',
    category: 'scenario',
    description: '云原生 · 灰度/回滚/暂停全流程',
    source: k8sRolloutState,
    featured: true,
  },
  {
    id: 'architecture-ai-agent-docops',
    title: 'AI Agent 文档自动化管线',
    kind: 'architecture',
    category: 'scenario',
    description: 'AI 工程 · 文档生成与审核流水线',
    source: aiAgentDocops,
    featured: true,
  },
  {
    id: 'architecture-microservices',
    title: '微服务架构',
    kind: 'architecture',
    category: 'basic',
    description: '服务、网关与数据库的拓扑',
    source: microservices,
    featured: true,
  },
  {
    id: 'architecture-three-tier',
    title: '三层架构',
    kind: 'architecture',
    category: 'basic',
    description: '经典前端-后端-数据库分层',
    source: threeTier,
    featured: true,
  },
  {
    id: 'architecture-data-pipeline',
    title: '数据仓 ETL 处理',
    kind: 'architecture',
    category: 'scenario',
    description: '数据可视化 · 数仓 ETL 拓扑',
    source: dataPipeline,
    featured: true,
  },
  {
    id: 'mindmap-ai-learning',
    title: 'AI 学习路线',
    kind: 'mindmap',
    category: 'scenario',
    description: '教育学习 · AI 领域知识体系',
    source: aiLearningRoadmap,
    featured: true,
  },
  {
    id: 'state-on-off',
    title: '开关状态',
    kind: 'state',
    category: 'basic',
    description: '最简单二态状态机示例',
    source: onOffState,
    featured: true,
  },
  {
    id: 'sequence-payment-gateway',
    title: '支付网关交互',
    kind: 'sequence',
    category: 'scenario',
    description: '电商交易 · 支付回调链路',
    source: paymentGateway,
    featured: true,
  },

  // ── 基础示例 ─────────────────────────────────────────────
  {
    id: 'flowchart-user-auth',
    title: '用户认证',
    kind: 'flowchart',
    category: 'basic',
    description: '登录鉴权的判定分支流程',
    source: userAuth,
  },
  {
    id: 'flowchart-order-approval',
    title: '订单审批',
    kind: 'flowchart',
    category: 'basic',
    description: '多级审批与驳回回环',
    source: orderApproval,
  },
  {
    id: 'sequence-oauth-login',
    title: 'OAuth 登录',
    kind: 'sequence',
    category: 'basic',
    description: '第三方授权的时序交互',
    source: oauthLogin,
  },
  {
    id: 'state-order-lifecycle',
    title: '订单生命周期',
    kind: 'state',
    category: 'basic',
    description: '订单状态机与状态转移',
    source: orderLifecycle,
  },
  {
    id: 'er-blog-schema',
    title: '博客 Schema',
    kind: 'er',
    category: 'basic',
    description: '实体关系与基数标注',
    source: blogSchema,
  },
  {
    id: 'mindmap-tech-stack',
    title: '技术栈',
    kind: 'mindmap',
    category: 'basic',
    description: '层级展开的知识树',
    source: techStack,
  },

  // ── 场景示例 ─────────────────────────────────────────────
  {
    id: 'flowchart-employee-onboarding',
    title: '员工入职流程',
    kind: 'flowchart',
    category: 'scenario',
    description: '办公协作 · 跨部门入职编排',
    source: employeeOnboarding,
  },
  {
    id: 'architecture-ecommerce',
    title: '电商平台全栈架构',
    kind: 'architecture',
    category: 'scenario',
    description: '分层分组 · 组内 edge 就近声明',
    source: ecommercePlatform,
  },
  {
    id: 'er-social-network',
    title: '社区互动数据模型',
    kind: 'er',
    category: 'scenario',
    description: '社交互动 · 关注/点赞/评论建模',
    source: socialNetwork,
  },
  {
    id: 'mindmap-programming-learning',
    title: '编程学习大纲',
    kind: 'mindmap',
    category: 'scenario',
    description: '教育学习 · 课程路径规划',
    source: programmingLearning,
  },

  // ── 压力测试 ─────────────────────────────────────────────
  {
    id: 'stress-flowchart-dag',
    title: '复杂 DAG 与长边路由',
    kind: 'flowchart',
    category: 'stress',
    description: '大规模有向无环图的分层布局',
    source: stressDag,
  },
  {
    id: 'stress-architecture-nested',
    title: '深度嵌套与跨组路由',
    kind: 'architecture',
    category: 'stress',
    description: '多层分组嵌套的连线穿越',
    source: stressNested,
  },
  {
    id: 'stress-er-dense',
    title: '高密度网状连线',
    kind: 'er',
    category: 'stress',
    description: '密集实体间的多对多关系',
    source: stressDense,
  },
  {
    id: 'stress-state-transitions',
    title: '复杂状态循环与自环',
    kind: 'state',
    category: 'stress',
    description: '含自环与回边的状态机',
    source: stressTransitions,
  },
  {
    id: 'stress-sequence-lifelines',
    title: '密集时序与长跨度回调',
    kind: 'sequence',
    category: 'stress',
    description: '多生命线的长跨度消息',
    source: stressLifelines,
  },
];

export const DEFAULT_EXAMPLE_ID = 'architecture-microservices';

export const CATEGORY_LABELS: Record<ExampleCategory, string> = {
  basic: '基础',
  scenario: '场景',
  stress: '压力测试',
};

export const KIND_LABELS: Record<DiagramKind, string> = {
  flowchart: '流程图',
  sequence: '时序图',
  architecture: '架构图',
  state: '状态图',
  er: 'ER 图',
  mindmap: '思维导图',
};

export function getExample(id: string): Example | undefined {
  return EXAMPLES.find((e) => e.id === id);
}
