// 客户退款流程：申请受理 → 审核 → 财务处理三段
// Mermaid 对照: 复杂电商退款流程图，含 group 分段与回环
diagram flowchart {
    title: "客户退款流程"
    config {
        direction: top-to-bottom
    }

    group intake "申请受理" {
        entity[start] request "提交退款申请"
        entity[process] classify "退款类型识别"
        entity[decision] classify_gate "是否符合政策？"
        entity[process] collect_evidence "补充凭证材料"
        entity[decision] evidence_gate "材料齐全？"
        entity[process] reject "拒绝退款"
        entity[end] rejected_end "申请关闭"
    }

    group review "审核阶段" {
        entity[process] cs_review "客服初审"
        entity[decision] cs_gate "客服通过？"
        entity[process] supervisor "主管复核"
        entity[decision] sup_gate "复核通过？"
        entity[process] fraud_check "反欺诈核查"
        entity[decision] fraud_gate "存在风险？"
        entity[process] escalate "升级风控团队"
        entity[process] back_to_cs "退回客服补充"
    }

    group finance "财务处理" {
        entity[process] approve "审批通过"
        entity[process] refund_calc "退款金额计算"
        entity[process] refund_pay "执行退款"
        entity[decision] pay_gate "退款到账？"
        entity[process] retry_pay "重新发起退款"
        entity[process] notify "通知客户"
        entity[process] archive "归档与对账"
        entity[end] done "退款完成"
    }

    request -> classify
    classify -> classify_gate
    classify_gate -> collect_evidence "是"
    classify_gate -> reject "否"
    reject -> rejected_end

    collect_evidence -> evidence_gate
    evidence_gate -> cs_review "是"
    evidence_gate -> collect_evidence "否，要求补齐"

    cs_review -> cs_gate
    cs_gate -> supervisor "是"
    cs_gate -> back_to_cs "否"
    back_to_cs -> cs_review "补充后重审"

    supervisor -> sup_gate
    sup_gate -> fraud_check "是"
    sup_gate -> back_to_cs "否"

    fraud_check -> fraud_gate
    fraud_gate -> escalate "是"
    fraud_gate -> approve "否"
    escalate -> reject "确认欺诈"

    approve -> refund_calc
    refund_calc -> refund_pay
    refund_pay -> pay_gate
    pay_gate -> notify "是"
    pay_gate -> retry_pay "否"
    retry_pay -> pay_gate "再次校验"

    notify -> archive
    archive -> done
}
