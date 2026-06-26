// Large K8s cluster overview with namespace to deployment aggregation
// Mermaid mapping: complex architecture graph with nested subgraphs
diagram architecture {
    title: "K8s 多命名空间生产集群总览"

    group edge "流量入口" {
        entity cdn "Global CDN" { type: external }
        entity waf "WAF" { type: gateway }
        entity ingress "Ingress Gateway" { type: gateway }
        entity dns "DNS" { type: external }
    }

    group payment_ns "payment-prod" {
        entity pay_gateway "pay-gateway x6" { type: service }
        entity pay_core "pay-core x18" { type: service }
        entity pay_risk "pay-risk x8" { type: service }
        entity pay_worker "pay-worker x24" { type: service }
    }

    group order_ns "order-prod" {
        entity order_api "order-api x10" { type: service }
        entity order_core "order-core x16" { type: service }
        entity order_fulfill "order-fulfillment x12" { type: service }
        entity order_worker "order-worker x20" { type: service }
    }

    group user_ns "user-prod" {
        entity user_api "user-api x8" { type: service }
        entity profile_svc "profile-service x6" { type: service }
        entity auth_svc "auth-service x12" { type: service }
        entity session_svc "session-service x10" { type: service }
    }

    group platform_ns "platform-system" {
        entity service_mesh "Service Mesh" { type: service }
        entity config_center "Config Center" { type: service }
        entity secrets "Secrets Manager" { type: service }
        entity argo "Argo CD" { type: service }
        entity metrics "Prometheus" { type: service }
        entity logs "Loki" { type: service }
    }

    group data_ns "stateful-data" {
        entity postgres "PostgreSQL Cluster" { type: database }
        entity redis "Redis Cluster" { type: cache }
        entity kafka "Kafka" { type: queue }
        entity object_store "Object Storage" { type: storage }
    }

    dns -> cdn
    cdn -> waf
    waf -> ingress
    ingress -> pay_gateway
    ingress -> order_api
    ingress -> user_api

    pay_gateway -> pay_core "sync payment"
    pay_gateway -> pay_risk "fraud check"
    pay_core -> pay_worker "async jobs"
    pay_core -> postgres
    pay_core -> redis
    pay_worker -> kafka

    order_api -> order_core
    order_core -> order_fulfill
    order_core -> postgres
    order_core -> kafka "order events"
    order_worker -> kafka
    order_fulfill -> object_store

    user_api -> profile_svc
    user_api -> auth_svc
    auth_svc -> session_svc
    auth_svc -> redis
    profile_svc -> postgres

    pay_core -> service_mesh
    order_core -> service_mesh
    user_api -> service_mesh
    service_mesh -> config_center
    service_mesh -> secrets
    argo -> pay_gateway "deploy"
    argo -> order_api "deploy"
    argo -> user_api "deploy"

    pay_core -> metrics
    order_core -> metrics
    user_api -> metrics
    pay_core -> logs
    order_core -> logs
    user_api -> logs
}
