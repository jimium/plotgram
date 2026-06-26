// Real-time risk decisioning for transaction authorization
// Mermaid mapping: complex sequence diagram for fraud and policy checks
diagram sequence {
    title: "实时风控决策链路"

    entity[actor] user "用户"
    entity[boundary] app "业务前端"
    entity[control] gateway "Transaction Gateway"
    entity[control] risk_api "Risk API"
    entity[database] feature_store "Feature Store"
    entity[control] rules "Rule Engine"
    entity[control] model "Model Scoring"
    entity[boundary] analyst "Risk Analyst Console"

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
