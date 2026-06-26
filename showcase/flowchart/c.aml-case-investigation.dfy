// AML case investigation workflow with screening, escalation, and reporting
// Mermaid mapping: complex compliance flowchart for anti-money laundering review
diagram flowchart {
    title: "反洗钱案件调查流程"
    config {
        direction: top-to-bottom
    }

    entity alert "命中可疑告警" { type: start }
    entity collect "收集交易与客户信息" { type: process }
    entity screen "名单与规则复筛" { type: process }
    entity false_gate "是否误报？" { type: decision }
    entity close_case "关闭案件" { type: process }
    entity analyst "人工调查" { type: process }
    entity risk_gate "是否高风险？" { type: decision }
    entity freeze "冻结账户或交易" { type: process }
    entity escalate "升级合规经理" { type: process }
    entity report_gate "需要监管报送？" { type: decision }
    entity sar "提交可疑报告" { type: process }
    entity archive "归档证据与结论" { type: process }
    entity done "调查完成" { type: end }

    alert -> collect
    collect -> screen
    screen -> false_gate
    false_gate -> close_case "是"
    false_gate -> analyst "否"
    close_case -> archive
    analyst -> risk_gate
    risk_gate -> freeze "是"
    risk_gate -> archive "否"
    freeze -> escalate
    escalate -> report_gate
    report_gate -> sar "是"
    report_gate -> archive "否"
    sar -> archive
    archive -> done
}
