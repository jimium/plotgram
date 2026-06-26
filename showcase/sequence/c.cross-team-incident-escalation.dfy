// Cross-team incident escalation from detection to coordinated mitigation
// Mermaid mapping: complex sequence diagram for enterprise incident response
diagram sequence {
    title: "跨团队故障升级协同"

    entity monitor "监控平台" { type: boundary }
    entity oncall "值班工程师" { type: actor }
    entity sre "SRE 团队" { type: control }
    entity app_team "应用团队" { type: control }
    entity db_team "数据库团队" { type: control }
    entity security "安全团队" { type: control }
    entity comms "应急沟通频道" { type: boundary }
    entity exec "值班经理" { type: actor }

    monitor --> oncall "critical alert"
    oncall -> comms "open incident bridge"
    oncall -> sre "request platform triage"
    sre --> comms "node and traffic status"
    oncall -> app_team "request app diagnostics"
    app_team --> comms "error spike confirmed"
    app_team -> db_team "suspect database latency"
    db_team --> comms "replica lag detected"
    oncall -> security "check for security anomaly"
    security --> comms "no active threat"
    sre -> app_team "apply rate limit and failover"
    app_team -> db_team "switch read traffic"
    db_team --> app_team "read path recovered"
    comms --> exec "incident update"
    exec --> comms "approve mitigation plan"
    app_team --> oncall "service stabilized"
}
