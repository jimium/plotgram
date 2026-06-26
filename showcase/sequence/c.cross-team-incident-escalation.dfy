// Cross-team incident escalation from detection to coordinated mitigation
// Mermaid mapping: complex sequence diagram for enterprise incident response
diagram sequence {
    title: "跨团队故障升级协同"

    entity[boundary] monitor "监控平台"
    entity[actor] oncall "值班工程师"
    entity[control] sre "SRE 团队"
    entity[control] app_team "应用团队"
    entity[control] db_team "数据库团队"
    entity[control] security "安全团队"
    entity[boundary] comms "应急沟通频道"
    entity[actor] exec "值班经理"

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
