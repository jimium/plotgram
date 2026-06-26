// Real-time risk decisioning for transaction authorization
// Mermaid mapping: complex sequence diagram for fraud and policy checks
diagram sequence {
    title: "实时风控决策链路"

    entity user "用户" { type: actor }
    entity app "业务前端" { type: boundary }
    entity gateway "Transaction Gateway" { type: control }
    entity risk_api "Risk API" { type: control }
    entity feature_store "Feature Store" { type: database }
    entity rules "Rule Engine" { type: control }
    entity model "Model Scoring" { type: control }
    entity analyst "Risk Analyst Console" { type: boundary }

    user -> app "submit transaction"
    app -> gateway "authorize request"
    gateway -> risk_api "risk decision request"
    risk_api -> feature_store "load user features"
    risk_api -> rules "evaluate hard rules"
    rules --> risk_api "rule result"
    risk_api -> model "score transaction"
    model --> risk_api "risk score"
    risk_api --> gateway "approve or challenge"
    gateway --> app "authorization result"
    risk_api -> analyst "high-risk case"
}
