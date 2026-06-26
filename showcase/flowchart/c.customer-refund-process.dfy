// 客户退款流程：申请受理 → 审核 → 财务处理三段
// Mermaid 对照: 复杂电商退款流程图，含 group 分段与回环
diagram flowchart {
    title: "客户退款流程"
    config {
        direction: top-to-bottom
    }

    group intake "申请受理" {
        entity request "提交退款申请" { type: start }
        entity classify "退款类型识别" { type: process }
        entity classify_gate "是否符合政策？" { type: decision }
        entity collect_evidence "补充凭证材料" { type: process }
        entity evidence_gate "材料齐全？" { type: decision }
        entity reject "拒绝退款" { type: process }
        entity rejected_end "申请关闭" { type: end }
    }

    group review "审核阶段" {
        entity cs_review "客服初审" { type: process }
        entity cs_gate "客服通过？" { type: decision }
        entity supervisor "主管复核" { type: process }
        entity sup_gate "复核通过？" { type: decision }
        entity fraud_check "反欺诈核查" { type: process }
        entity fraud_gate "存在风险？" { type: decision }
        entity escalate "升级风控团队" { type: process }
        entity back_to_cs "退回客服补充" { type: process }
    }

    group finance "财务处理" {
        entity approve "审批通过" { type: process }
        entity refund_calc "退款金额计算" { type: process }
        entity refund_pay "执行退款" { type: process }
        entity pay_gate "退款到账？" { type: decision }
        entity retry_pay "重新发起退款" { type: process }
        entity notify "通知客户" { type: process }
        entity archive "归档与对账" { type: process }
        entity done "退款完成" { type: end }
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
