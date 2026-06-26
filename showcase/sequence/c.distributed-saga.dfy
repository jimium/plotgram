// 分布式 Saga：跨服务事务 + 补偿
// Mermaid 对照: 复杂 sequenceDiagram（含补偿路径）
diagram sequence {
    title: "分布式 Saga 事务"

    entity[control] orchestrator "编排器"
    entity[control] order "订单服务"
    entity[control] payment "支付服务"
    entity[control] shipping "物流服务"

    orchestrator -> order "创建订单"
    order --> orchestrator "订单已创建"
    orchestrator -> payment "扣款"
    payment --> orchestrator "扣款成功"
    orchestrator -> shipping "创建运单"
    shipping --> orchestrator "运单失败"
    orchestrator -> payment "退款补偿"
    payment --> orchestrator "退款完成"
    orchestrator -> order "取消订单"
    order --> orchestrator "订单已取消"
}
