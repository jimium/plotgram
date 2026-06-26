// Hybrid cloud disaster recovery topology across on-prem and public cloud
// Mermaid mapping: complex architecture graph for compliance and DR planning
diagram architecture {
    title: "混合云灾备拓扑"

    group internet "互联网入口" {
        entity dns "Authoritative DNS" { type: external }
        entity traffic_mgr "Traffic Manager" { type: gateway }
        entity waf "WAF" { type: gateway }
    }

    group primary_dc "主数据中心" {
        entity primary_ingress "Primary Ingress" { type: gateway }
        entity primary_app "core-app x16" { type: service }
        entity primary_risk "risk-engine x10" { type: service }
        entity primary_batch "settlement-batch x8" { type: service }
        entity primary_db "Primary Oracle" { type: database }
    }

    group dr_cloud "公有云灾备" {
        entity dr_ingress "DR Ingress" { type: gateway }
        entity dr_app "core-app-dr x8" { type: service }
        entity dr_risk "risk-engine-dr x6" { type: service }
        entity dr_batch "settlement-batch-dr x4" { type: service }
        entity dr_db "DR PostgreSQL" { type: database }
    }

    group governance "治理与合规" {
        entity iam "Central IAM" { type: service }
        entity cmdb "CMDB" { type: service }
        entity audit "Audit Archive" { type: storage }
        entity observability "Unified Observability" { type: service }
    }

    dns -> traffic_mgr
    traffic_mgr -> waf
    waf -> primary_ingress "primary route"
    waf -> dr_ingress "dr failover"

    primary_ingress -> primary_app
    primary_app -> primary_risk
    primary_app -> primary_batch
    primary_app -> primary_db
    primary_risk -> primary_db

    dr_ingress -> dr_app
    dr_app -> dr_risk
    dr_app -> dr_batch
    dr_app -> dr_db
    dr_risk -> dr_db

    iam -> primary_app
    iam -> dr_app
    cmdb -> primary_app "inventory sync"
    cmdb -> dr_app "inventory sync"
    primary_app -> observability
    dr_app -> observability
    primary_batch -> audit "daily evidence"
    dr_batch -> audit "dr evidence"

    primary_db --> dr_db "async replication"
    primary_app --> dr_app "configuration sync"
}
