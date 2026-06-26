// 订单生命周期
// Mermaid 对照: stateDiagram-v2 订单状态流转
diagram state {
    title: "订单生命周期"

    entity[initial] init ""
    entity[state] created "已创建"
    entity[state] paid "已支付"
    entity[state] shipped "已发货"
    entity[state] delivered "已签收"
    entity[final] completed "已完成"
    entity[final] cancelled "已取消"
    entity[choice] timeout "支付超时"

    init -> created
    created -> timeout
    timeout -> paid "用户支付"
    timeout -> cancelled "超时未付"
    paid -> shipped "仓库发货"
    shipped -> delivered "物流签收"
    delivered -> completed
    paid -> cancelled "用户取消"
}
