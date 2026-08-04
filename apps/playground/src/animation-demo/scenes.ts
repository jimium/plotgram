export interface AnimationScene {
  id: string;
  title: string;
  description: string;
  dsls: string[];
  frameLabels: string[];
}

export const SCENES: AnimationScene[] = [
  {
    id: 'microservices',
    title: '微服务架构演化',
    description: '从单体应用到微服务架构的逐步拆分过程，展示节点添加与边的动态变化',
    frameLabels: ['单体应用', '接入负载均衡', '拆分核心服务', '引入消息队列', '添加缓存层', '完整微服务'],
    dsls: [
      `diagram architecture {
  title: "微服务架构演化"
  entity[frontend] client "用户端"
  entity[service] app "单体应用"
  entity[database] db "主数据库"
  client -> app "请求"
  app -> db "读写"
}`,
      `diagram architecture {
  title: "微服务架构演化"
  entity[frontend] client "用户端"
  entity[gateway] lb "负载均衡"
  entity[service] app "单体应用"
  entity[database] db "主数据库"
  client -> lb "HTTPS"
  lb -> app "转发"
  app -> db "读写"
}`,
      `diagram architecture {
  title: "微服务架构演化"
  entity[frontend] client "用户端"
  entity[gateway] lb "负载均衡"
  entity[service] user "用户服务"
  entity[service] order "订单服务"
  entity[service] product "商品服务"
  entity[database] db "主数据库"
  client -> lb "HTTPS"
  lb -> user
  lb -> order
  lb -> product
  user -> db
  order -> db
  product -> db
}`,
      `diagram architecture {
  title: "微服务架构演化"
  entity[frontend] client "用户端"
  entity[gateway] lb "负载均衡"
  entity[service] user "用户服务"
  entity[service] order "订单服务"
  entity[service] product "商品服务"
  entity[queue] mq "消息队列"
  entity[database] db "主数据库"
  client -> lb "HTTPS"
  lb -> user
  lb -> order
  lb -> product
  user -> db
  product -> db
  order -> mq "发布事件"
  mq --> user "消费"
  mq --> product "消费"
}`,
      `diagram architecture {
  title: "微服务架构演化"
  entity[frontend] client "用户端"
  entity[gateway] lb "负载均衡"
  entity[service] user "用户服务"
  entity[service] order "订单服务"
  entity[service] product "商品服务"
  entity[queue] mq "消息队列"
  entity[cache] cache "Redis 缓存"
  entity[database] db "主数据库"
  client -> lb "HTTPS"
  lb -> user
  lb -> order
  lb -> product
  user -> db
  user -> cache
  product -> cache
  product -> db
  order -> mq "发布事件"
  mq --> user "消费"
  mq --> product "消费"
}`,
      `diagram architecture {
  title: "微服务架构演化"
  entity[frontend] client "用户端"
  entity[external] cdn "CDN"
  entity[gateway] waf "WAF"
  entity[gateway] lb "负载均衡"
  entity[service] user "用户服务"
  entity[service] order "订单服务"
  entity[service] product "商品服务"
  entity[service] payment "支付服务"
  entity[queue] mq "Kafka"
  entity[cache] cache "Redis 集群"
  entity[database] user_db "用户库"
  entity[database] order_db "订单库"
  entity[database] product_db "商品库"
  entity[storage] s3 "对象存储"
  client -> cdn "静态资源"
  cdn -> waf
  waf -> lb "HTTPS"
  lb -> user
  lb -> order
  lb -> product
  lb -> payment
  user -> user_db
  user -> cache
  order -> order_db
  order -> mq "订单事件"
  product -> product_db
  product -> cache
  product -> s3 "图片"
  payment -> mq "支付事件"
  mq --> user
  mq --> product
  mq --> order
}`,
    ],
  },
  {
    id: 'cicd',
    title: 'CI/CD 流水线构建',
    description: '从简单部署到完整 CI/CD 流水线的渐进式搭建',
    frameLabels: ['手动部署', '添加 CI 构建', '自动化测试', '多环境部署', '完整 DevOps 流水线'],
    dsls: [
      `diagram flowchart {
  title: "CI/CD 流水线"
  config { direction: top-to-bottom }
  entity[start] dev "开发者"
  entity[process] deploy "手动部署"
  entity[end] prod "生产环境"
  dev -> deploy "git push"
  deploy -> prod
}`,
      `diagram flowchart {
  title: "CI/CD 流水线"
  config { direction: top-to-bottom }
  entity[start] dev "开发者"
  entity[process] push "推送代码"
  entity[process] ci "CI 构建"
  entity[process] deploy "部署"
  entity[end] prod "生产环境"
  dev -> push
  push -> ci "触发"
  ci -> deploy
  deploy -> prod
}`,
      `diagram flowchart {
  title: "CI/CD 流水线"
  config { direction: top-to-bottom }
  entity[start] dev "开发者"
  entity[process] push "推送代码"
  entity[process] ci "CI 构建"
  entity[decision] test "自动化测试"
  entity[process] fix "修复问题"
  entity[process] deploy "部署"
  entity[end] prod "生产环境"
  dev -> push
  push -> ci
  ci -> test "构建完成"
  test -> deploy "通过"
  test -> fix "失败"
  fix -> push "重新提交"
  deploy -> prod
}`,
      `diagram flowchart {
  title: "CI/CD 流水线"
  config { direction: top-to-bottom }
  entity[start] dev "开发者"
  entity[process] push "推送代码"
  entity[process] ci "CI 构建"
  entity[decision] test "自动化测试"
  entity[process] fix "修复问题"
  entity[process] staging "部署 Staging"
  entity[decision] approval "人工审核"
  entity[process] prod_deploy "部署生产"
  entity[process] staging_env "Staging 环境"
  entity[end] prod "生产环境"
  dev -> push
  push -> ci
  ci -> test
  test -> staging "通过"
  test -> fix "失败"
  fix -> push
  staging -> staging_env
  staging -> approval
  approval -> prod_deploy "批准"
  approval -> fix "拒绝"
  prod_deploy -> prod
}`,
      `diagram flowchart {
  title: "CI/CD 流水线"
  config { direction: top-to-bottom }
  entity[start] dev "开发者"
  entity[process] pr "Pull Request"
  entity[decision] review "代码审查"
  entity[process] merge "合并主分支"
  entity[process] ci "CI 构建"
  entity[process] lint "Lint 检查"
  entity[process] unit "单元测试"
  entity[process] integration "集成测试"
  entity[process] fix "修复问题"
  entity[process] build "构建镜像"
  entity[database] registry "镜像仓库"
  entity[process] staging "Staging 部署"
  entity[decision] e2e "E2E 测试"
  entity[decision] approval "生产审批"
  entity[process] canary "金丝雀发布"
  entity[process] prod "全量发布"
  entity[end] prod_env "生产环境"
  entity[process] monitor "监控告警"
  entity[process] rollback "回滚"
  dev -> pr
  pr -> review
  review -> merge "通过"
  review -> fix "需要修改"
  fix -> pr
  merge -> ci "触发"
  ci -> lint
  lint -> unit
  unit -> integration
  integration -> build "全部通过"
  integration -> fix "失败"
  build -> registry "推送"
  registry -> staging
  staging -> e2e
  e2e -> approval "通过"
  e2e -> fix "失败"
  approval -> canary "批准"
  canary -> monitor "观察"
  monitor -> prod "指标正常"
  monitor -> rollback "异常"
  rollback -> staging
  prod -> prod_env
}`,
    ],
  },
  {
    id: 'state-machine',
    title: '订单状态机演化',
    description: '订单状态从简单流转到包含分支、取消、退款的复杂状态机',
    frameLabels: ['基础三态', '添加支付状态', '添加发货环节', '支持取消与退款', '完整订单状态机'],
    dsls: [
      `diagram state {
  title: "订单状态机"
  entity[initial] init ""
  entity[state] pending "待处理"
  entity[final] done "已完成"
  init -> pending
  pending -> done "处理"
}`,
      `diagram state {
  title: "订单状态机"
  entity[initial] init ""
  entity[state] created "已创建"
  entity[state] paid "已支付"
  entity[final] done "已完成"
  init -> created
  created -> paid "支付"
  paid -> done "完成"
}`,
      `diagram state {
  title: "订单状态机"
  entity[initial] init ""
  entity[state] created "已创建"
  entity[state] paid "已支付"
  entity[state] shipped "已发货"
  entity[state] delivered "已送达"
  entity[final] done "已完成"
  init -> created
  created -> paid "支付"
  paid -> shipped "发货"
  shipped -> delivered "送达"
  delivered -> done "确认收货"
}`,
      `diagram state {
  title: "订单状态机"
  entity[initial] init ""
  entity[state] created "已创建"
  entity[state] paid "已支付"
  entity[state] shipped "已发货"
  entity[state] delivered "已送达"
  entity[final] done "已完成"
  entity[final] cancelled "已取消"
  entity[state] refund "退款中"
  entity[final] refunded "已退款"
  init -> created
  created -> paid "支付"
  created -> cancelled "取消"
  paid -> shipped "发货"
  paid -> cancelled "取消订单"
  paid -> refund "申请退款"
  shipped -> delivered "送达"
  shipped -> refund "申请退款"
  delivered -> done "确认收货"
  refund -> refunded "退款完成"
  refund -> shipped "退款驳回"
}`,
      `diagram state {
  title: "订单状态机"
  entity[initial] init ""
  entity[state] created "已创建"
  entity[state] paying "支付中"
  entity[state] paid "已支付"
  entity[state] packing "备货中"
  entity[state] shipped "已发货"
  entity[state] delivered "已送达"
  entity[final] done "已完成"
  entity[final] cancelled "已取消"
  entity[state] refund_apply "退款申请"
  entity[choice] refund_review "退款审核"
  entity[state] refunding "退款中"
  entity[final] refunded "已退款"
  entity[state] ret "退货中"
  entity[final] returned "已退货"
  init -> created
  created -> paying "发起支付"
  paying -> paid "支付成功"
  paying -> created "支付失败"
  paying -> cancelled "超时取消"
  created -> cancelled "用户取消"
  paid -> packing
  paid -> cancelled "取消订单"
  paid -> refund_apply "申请退款"
  packing -> shipped "发货"
  shipped -> delivered "送达"
  delivered -> done "确认收货"
  delivered -> ret "申请退货"
  delivered -> refund_apply "申请退款"
  refund_apply -> refund_review
  refund_review -> refunding "同意退款"
  refund_review -> shipped "拒绝"
  refunding -> refunded "退款完成"
  ret -> refund_review
  refund_review -> ret "同意退货"
  ret -> returned "退货完成"
  ret -> delivered "拒绝"
}`,
    ],
  },
  {
    id: 'mindmap',
    title: '思维导图展开',
    description: '以思维导图形式展示产品功能从核心概念到完整特性的展开过程',
    frameLabels: ['核心概念', '一级分支', '主要功能展开', '详细特性', '完整产品地图'],
    dsls: [
      `diagram mindmap {
  title: "产品规划"
  entity[root] root "FlowML"
}`,
      `diagram mindmap {
  title: "产品规划"
  entity[root] root "FlowML"
  entity[main] core "核心引擎"
  entity[main] dsl "DSL 语言"
  entity[main] render "渲染输出"
  root -> core
  root -> dsl
  root -> render
}`,
      `diagram mindmap {
  title: "产品规划"
  entity[root] root "FlowML"
  entity[main] core "核心引擎"
  entity[main] dsl "DSL 语言"
  entity[main] render "渲染输出"
  entity[branch] layout "布局算法"
  entity[branch] routing "边路由"
  entity[branch] parser "解析器"
  entity[branch] svg "SVG"
  entity[branch] drawio "Draw.io"
  root -> core
  root -> dsl
  root -> render
  core -> layout
  core -> routing
  dsl -> parser
  render -> svg
  render -> drawio
}`,
      `diagram mindmap {
  title: "产品规划"
  entity[root] root "FlowML"
  entity[main] core "核心引擎"
  entity[main] dsl "DSL 语言"
  entity[main] render "渲染输出"
  entity[main] tooling "工具链"
  entity[branch] layout "布局算法"
  entity[branch] routing "边路由"
  entity[branch] parser "解析器"
  entity[branch] validate "语义校验"
  entity[branch] svg "SVG"
  entity[branch] drawio "Draw.io"
  entity[branch] ascii "ASCII"
  entity[branch] wasm "WASM"
  entity[branch] cli "CLI"
  entity[leaf] sugiyama "Sugiyama"
  entity[leaf] force "Force-Directed"
  entity[leaf] orthogonal "Orthogonal"
  entity[leaf] bezier "Bezier"
  root -> core
  root -> dsl
  root -> render
  root -> tooling
  core -> layout
  core -> routing
  dsl -> parser
  dsl -> validate
  render -> svg
  render -> drawio
  render -> ascii
  tooling -> wasm
  tooling -> cli
  layout -> sugiyama
  layout -> force
  routing -> orthogonal
  routing -> bezier
}`,
      `diagram mindmap {
  title: "产品规划"
  entity[root] root "FlowML"
  entity[main] core "核心引擎"
  entity[main] dsl "DSL 语言"
  entity[main] render "渲染输出"
  entity[main] tooling "工具链"
  entity[main] themes "主题样式"
  entity[branch] layout "布局算法"
  entity[branch] routing "边路由"
  entity[branch] bundling "边聚合"
  entity[branch] parser "解析器"
  entity[branch] validate "语义校验"
  entity[branch] diff "Diff/Patch"
  entity[branch] svg "SVG"
  entity[branch] drawio "Draw.io"
  entity[branch] ascii "ASCII Art"
  entity[branch] png "PNG"
  entity[branch] wasm "WASM"
  entity[branch] cli "CLI"
  entity[branch] playground "Playground"
  entity[branch] light "浅色主题"
  entity[branch] dark "深色主题"
  entity[branch] hand "手绘风格"
  entity[leaf] l_sugiyama "Sugiyama 分层"
  entity[leaf] l_force "Force-Directed"
  entity[leaf] l_circular "Circular 环形"
  entity[leaf] l_mindmap "Mindmap 树状"
  entity[leaf] r_ortho "Orthogonal 正交"
  entity[leaf] r_bezier "Bezier 贝塞尔"
  entity[leaf] r_spline "Spline 样条"
  entity[leaf] r_organic "Organic 自然"
  entity[leaf] b_trunk "主干聚合"
  entity[leaf] b_channel "通道分流"
  entity[leaf] d_semantic "语义比较"
  entity[leaf] d_patch "增量补丁"
  entity[leaf] d_anim "动画过渡"
  entity[leaf] p_live "实时编辑"
  entity[leaf] p_anim "动画演示"
  root -> core
  root -> dsl
  root -> render
  root -> tooling
  root -> themes
  core -> layout
  core -> routing
  core -> bundling
  dsl -> parser
  dsl -> validate
  dsl -> diff
  render -> svg
  render -> drawio
  render -> ascii
  render -> png
  tooling -> wasm
  tooling -> cli
  tooling -> playground
  themes -> light
  themes -> dark
  themes -> hand
  layout -> l_sugiyama
  layout -> l_force
  layout -> l_circular
  layout -> l_mindmap
  routing -> r_ortho
  routing -> r_bezier
  routing -> r_spline
  routing -> r_organic
  bundling -> b_trunk
  bundling -> b_channel
  diff -> d_semantic
  diff -> d_patch
  diff -> d_anim
  playground -> p_live
  playground -> p_anim
}`,
    ],
  },
];
