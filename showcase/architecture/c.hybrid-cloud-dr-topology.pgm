// Hybrid cloud disaster recovery topology across on-prem and public cloud
// Mermaid mapping: complex architecture graph for compliance and DR planning
diagram architecture {
    title: "混合云灾备拓扑"

    group internet "互联网入口" {
        entity[external] dns "Authoritative DNS"
        entity[gateway] traffic_mgr "Traffic Manager"
        entity[gateway] waf "WAF"
    }

    group primary_dc "主数据中心" {
        entity[gateway] primary_ingress "Primary Ingress"
        entity[service] primary_app "core-app x16"
        entity[service] primary_risk "risk-engine x10"
        entity[service] primary_batch "settlement-batch x8"
        entity[database] primary_db "Primary Oracle"
    }

    group dr_cloud "公有云灾备" {
        entity[gateway] dr_ingress "DR Ingress"
        entity[service] dr_app "core-app-dr x8"
        entity[service] dr_risk "risk-engine-dr x6"
        entity[service] dr_batch "settlement-batch-dr x4"
        entity[database] dr_db "DR PostgreSQL"
    }

    group governance "治理与合规" {
        entity[service] iam "Central IAM"
        entity[service] cmdb "CMDB"
        entity[storage] audit "Audit Archive"
        entity[service] observability "Unified Observability"
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
