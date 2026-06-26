// 订单生命周期
// Mermaid 对照: stateDiagram-v2 订单状态流转
diagram state {
    title: "订单生命周期"

    entity init "" { type: initial }
    entity created "已创建" { type: state }
    entity paid "已支付" { type: state }
    entity shipped "已发货" { type: state }
    entity delivered "已签收" { type: state }
    entity completed "已完成" { type: final }
    entity cancelled "已取消" { type: final }
    entity timeout "支付超时" { type: choice }

    init -> created
    created -> timeout
    timeout -> paid "用户支付"
    timeout -> cancelled "超时未付"
    paid -> shipped "仓库发货"
    shipped -> delivered "物流签收"
    delivered -> completed
    paid -> cancelled "用户取消"
}
