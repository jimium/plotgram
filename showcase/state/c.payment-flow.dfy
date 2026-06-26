// 支付状态机：含失败重试与退款
// Mermaid 对照: 复杂 stateDiagram-v2
diagram state {
    title: "支付状态机"

    entity[initial] init "初始化"
    entity[state] pending "待支付"
    entity[state] processing "支付处理中"
    entity[final] success "支付成功"
    entity[state] failed "支付失败"
    entity[state] retry "重试中"
    entity[state] refunding "退款中"
    entity[final] refunded "已退款"
    entity[final] expired "已过期"
    entity[choice] check_retry "是否可重试"

    init -> pending
    pending -> processing "用户确认支付"
    pending -> expired "超时"
    processing -> success "渠道返回成功"
    processing -> failed "渠道返回失败"
    failed -> check_retry
    check_retry -> retry "次数未超限"
    check_retry -> expired "次数已超限"
    retry -> processing
    success -> refunding "用户申请退款"
    refunding -> refunded "退款完成"
    refunding -> success "退款失败，保持成功"
}
