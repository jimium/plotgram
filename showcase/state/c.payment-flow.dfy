// 支付状态机：含失败重试与退款
// Mermaid 对照: 复杂 stateDiagram-v2
diagram state {
    title: "支付状态机"

    entity init "初始化" { type: initial }
    entity pending "待支付" { type: state }
    entity processing "支付处理中" { type: state }
    entity success "支付成功" { type: final }
    entity failed "支付失败" { type: state }
    entity retry "重试中" { type: state }
    entity refunding "退款中" { type: state }
    entity refunded "已退款" { type: final }
    entity expired "已过期" { type: final }
    entity check_retry "是否可重试" { type: choice }

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
