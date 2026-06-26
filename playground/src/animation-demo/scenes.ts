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
  entity client "用户端" { type: frontend }
  entity app "单体应用" { type: service }
  entity db "主数据库" { type: database }
  client -> app "请求"
  app -> db "读写"
}`,
      `diagram architecture {
  title: "微服务架构演化"
  entity client "用户端" { type: frontend }
  entity lb "负载均衡" { type: gateway }
  entity app "单体应用" { type: service }
  entity db "主数据库" { type: database }
  client -> lb "HTTPS"
  lb -> app "转发"
  app -> db "读写"
}`,
      `diagram architecture {
  title: "微服务架构演化"
  entity client "用户端" { type: frontend }
  entity lb "负载均衡" { type: gateway }
  entity user "用户服务" { type: service }
  entity order "订单服务" { type: service }
  entity product "商品服务" { type: service }
  entity db "主数据库" { type: database }
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
  entity client "用户端" { type: frontend }
  entity lb "负载均衡" { type: gateway }
  entity user "用户服务" { type: service }
  entity order "订单服务" { type: service }
  entity product "商品服务" { type: service }
  entity mq "消息队列" { type: queue }
  entity db "主数据库" { type: database }
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
  entity client "用户端" { type: frontend }
  entity lb "负载均衡" { type: gateway }
  entity user "用户服务" { type: service }
  entity order "订单服务" { type: service }
  entity product "商品服务" { type: service }
  entity mq "消息队列" { type: queue }
  entity cache "Redis 缓存" { type: cache }
  entity db "主数据库" { type: database }
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
  entity client "用户端" { type: frontend }
  entity cdn "CDN" { type: external }
  entity waf "WAF" { type: gateway }
  entity lb "负载均衡" { type: gateway }
  entity user "用户服务" { type: service }
  entity order "订单服务" { type: service }
  entity product "商品服务" { type: service }
  entity payment "支付服务" { type: service }
  entity mq "Kafka" { type: queue }
  entity cache "Redis 集群" { type: cache }
  entity user_db "用户库" { type: database }
  entity order_db "订单库" { type: database }
  entity product_db "商品库" { type: database }
  entity s3 "对象存储" { type: storage }
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
  entity dev "开发者" { type: start }
  entity deploy "手动部署" { type: process }
  entity prod "生产环境" { type: end }
  dev -> deploy "git push"
  deploy -> prod
}`,
      `diagram flowchart {
  title: "CI/CD 流水线"
  config { direction: top-to-bottom }
  entity dev "开发者" { type: start }
  entity push "推送代码" { type: process }
  entity ci "CI 构建" { type: process }
  entity deploy "部署" { type: process }
  entity prod "生产环境" { type: end }
  dev -> push
  push -> ci "触发"
  ci -> deploy
  deploy -> prod
}`,
      `diagram flowchart {
  title: "CI/CD 流水线"
  config { direction: top-to-bottom }
  entity dev "开发者" { type: start }
  entity push "推送代码" { type: process }
  entity ci "CI 构建" { type: process }
  entity test "自动化测试" { type: decision }
  entity fix "修复问题" { type: process }
  entity deploy "部署" { type: process }
  entity prod "生产环境" { type: end }
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
  entity dev "开发者" { type: start }
  entity push "推送代码" { type: process }
  entity ci "CI 构建" { type: process }
  entity test "自动化测试" { type: decision }
  entity fix "修复问题" { type: process }
  entity staging "部署 Staging" { type: process }
  entity approval "人工审核" { type: decision }
  entity prod_deploy "部署生产" { type: process }
  entity staging_env "Staging 环境" { type: process }
  entity prod "生产环境" { type: end }
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
  entity dev "开发者" { type: start }
  entity pr "Pull Request" { type: process }
  entity review "代码审查" { type: decision }
  entity merge "合并主分支" { type: process }
  entity ci "CI 构建" { type: process }
  entity lint "Lint 检查" { type: process }
  entity unit "单元测试" { type: process }
  entity integration "集成测试" { type: process }
  entity fix "修复问题" { type: process }
  entity build "构建镜像" { type: process }
  entity registry "镜像仓库" { type: database }
  entity staging "Staging 部署" { type: process }
  entity e2e "E2E 测试" { type: decision }
  entity approval "生产审批" { type: decision }
  entity canary "金丝雀发布" { type: process }
  entity prod "全量发布" { type: process }
  entity prod_env "生产环境" { type: end }
  entity monitor "监控告警" { type: process }
  entity rollback "回滚" { type: process }
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
  entity init "" { type: initial }
  entity pending "待处理" { type: state }
  entity done "已完成" { type: final }
  init -> pending
  pending -> done "处理"
}`,
      `diagram state {
  title: "订单状态机"
  entity init "" { type: initial }
  entity created "已创建" { type: state }
  entity paid "已支付" { type: state }
  entity done "已完成" { type: final }
  init -> created
  created -> paid "支付"
  paid -> done "完成"
}`,
      `diagram state {
  title: "订单状态机"
  entity init "" { type: initial }
  entity created "已创建" { type: state }
  entity paid "已支付" { type: state }
  entity shipped "已发货" { type: state }
  entity delivered "已送达" { type: state }
  entity done "已完成" { type: final }
  init -> created
  created -> paid "支付"
  paid -> shipped "发货"
  shipped -> delivered "送达"
  delivered -> done "确认收货"
}`,
      `diagram state {
  title: "订单状态机"
  entity init "" { type: initial }
  entity created "已创建" { type: state }
  entity paid "已支付" { type: state }
  entity shipped "已发货" { type: state }
  entity delivered "已送达" { type: state }
  entity done "已完成" { type: final }
  entity cancelled "已取消" { type: final }
  entity refund "退款中" { type: state }
  entity refunded "已退款" { type: final }
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
  entity init "" { type: initial }
  entity created "已创建" { type: state }
  entity paying "支付中" { type: state }
  entity paid "已支付" { type: state }
  entity packing "备货中" { type: state }
  entity shipped "已发货" { type: state }
  entity delivered "已送达" { type: state }
  entity done "已完成" { type: final }
  entity cancelled "已取消" { type: final }
  entity refund_apply "退款申请" { type: state }
  entity refund_review "退款审核" { type: choice }
  entity refunding "退款中" { type: state }
  entity refunded "已退款" { type: final }
  entity ret "退货中" { type: state }
  entity returned "已退货" { type: final }
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
  entity root "FlowML" { type: root }
}`,
      `diagram mindmap {
  title: "产品规划"
  entity root "FlowML" { type: root }
  entity core "核心引擎" { type: main }
  entity dsl "DSL 语言" { type: main }
  entity render "渲染输出" { type: main }
  root -> core
  root -> dsl
  root -> render
}`,
      `diagram mindmap {
  title: "产品规划"
  entity root "FlowML" { type: root }
  entity core "核心引擎" { type: main }
  entity dsl "DSL 语言" { type: main }
  entity render "渲染输出" { type: main }
  entity layout "布局算法" { type: branch }
  entity routing "边路由" { type: branch }
  entity parser "解析器" { type: branch }
  entity svg "SVG" { type: branch }
  entity drawio "Draw.io" { type: branch }
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
  entity root "FlowML" { type: root }
  entity core "核心引擎" { type: main }
  entity dsl "DSL 语言" { type: main }
  entity render "渲染输出" { type: main }
  entity tooling "工具链" { type: main }
  entity layout "布局算法" { type: branch }
  entity routing "边路由" { type: branch }
  entity parser "解析器" { type: branch }
  entity validate "语义校验" { type: branch }
  entity svg "SVG" { type: branch }
  entity drawio "Draw.io" { type: branch }
  entity ascii "ASCII" { type: branch }
  entity wasm "WASM" { type: branch }
  entity cli "CLI" { type: branch }
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
  layout -> "Sugiyama" { type: leaf }
  layout -> "Force-Directed" { type: leaf }
  routing -> "Orthogonal" { type: leaf }
  routing -> "Bezier" { type: leaf }
}`,
      `diagram mindmap {
  title: "产品规划"
  entity root "FlowML" { type: root }
  entity core "核心引擎" { type: main }
  entity dsl "DSL 语言" { type: main }
  entity render "渲染输出" { type: main }
  entity tooling "工具链" { type: main }
  entity themes "主题样式" { type: main }
  entity layout "布局算法" { type: branch }
  entity routing "边路由" { type: branch }
  entity bundling "边聚合" { type: branch }
  entity parser "解析器" { type: branch }
  entity validate "语义校验" { type: branch }
  entity diff "Diff/Patch" { type: branch }
  entity svg "SVG" { type: branch }
  entity drawio "Draw.io" { type: branch }
  entity ascii "ASCII Art" { type: branch }
  entity png "PNG" { type: branch }
  entity wasm "WASM" { type: branch }
  entity cli "CLI" { type: branch }
  entity playground "Playground" { type: branch }
  entity light "浅色主题" { type: branch }
  entity dark "深色主题" { type: branch }
  entity hand "手绘风格" { type: branch }
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
  layout -> "Sugiyama 分层" { type: leaf }
  layout -> "Force-Directed" { type: leaf }
  layout -> "Circular 环形" { type: leaf }
  layout -> "Mindmap 树状" { type: leaf }
  routing -> "Orthogonal 正交" { type: leaf }
  routing -> "Bezier 贝塞尔" { type: leaf }
  routing -> "Spline 样条" { type: leaf }
  routing -> "Organic 自然" { type: leaf }
  bundling -> "主干聚合" { type: leaf }
  bundling -> "通道分流" { type: leaf }
  diff -> "语义比较" { type: leaf }
  diff -> "增量补丁" { type: leaf }
  diff -> "动画过渡" { type: leaf }
  playground -> "实时编辑" { type: leaf }
  playground -> "动画演示" { type: leaf }
}`,
    ],
  },
];
