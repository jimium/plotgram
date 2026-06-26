// Large K8s cluster overview with namespace to deployment aggregation
// Mermaid mapping: complex architecture graph with nested subgraphs
diagram architecture {
    title: "K8s 多命名空间生产集群总览"

    group edge "流量入口" {
        entity[external] cdn "Global CDN"
        entity[gateway] waf "WAF"
        entity[gateway] ingress "Ingress Gateway"
        entity[external] dns "DNS"
    }

    group payment_ns "payment-prod" {
        entity[service] pay_gateway "pay-gateway x6"
        entity[service] pay_core "pay-core x18"
        entity[service] pay_risk "pay-risk x8"
        entity[service] pay_worker "pay-worker x24"
    }

    group order_ns "order-prod" {
        entity[service] order_api "order-api x10"
        entity[service] order_core "order-core x16"
        entity[service] order_fulfill "order-fulfillment x12"
        entity[service] order_worker "order-worker x20"
    }

    group user_ns "user-prod" {
        entity[service] user_api "user-api x8"
        entity[service] profile_svc "profile-service x6"
        entity[service] auth_svc "auth-service x12"
        entity[service] session_svc "session-service x10"
    }

    group platform_ns "platform-system" {
        entity[service] service_mesh "Service Mesh"
        entity[service] config_center "Config Center"
        entity[service] secrets "Secrets Manager"
        entity[service] argo "Argo CD"
        entity[service] metrics "Prometheus"
        entity[service] logs "Loki"
    }

    group data_ns "stateful-data" {
        entity[database] postgres "PostgreSQL Cluster"
        entity[cache] redis "Redis Cluster"
        entity[queue] kafka "Kafka"
        entity[storage] object_store "Object Storage"
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
