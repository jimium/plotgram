// AML case investigation workflow with screening, escalation, and reporting
// Mermaid mapping: complex compliance flowchart for anti-money laundering review
diagram flowchart {
    title: "反洗钱案件调查流程"
    config {
        direction: top-to-bottom
    }

    entity[start] alert "命中可疑告警"
    entity[process] collect "收集交易与客户信息"
    entity[process] screen "名单与规则复筛"
    entity[decision] false_gate "是否误报？"
    entity[process] close_case "关闭案件"
    entity[process] analyst "人工调查"
    entity[decision] risk_gate "是否高风险？"
    entity[process] freeze "冻结账户或交易"
    entity[process] escalate "升级合规经理"
    entity[decision] report_gate "需要监管报送？"
    entity[process] sar "提交可疑报告"
    entity[process] archive "归档证据与结论"
    entity[end] done "调查完成"

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
