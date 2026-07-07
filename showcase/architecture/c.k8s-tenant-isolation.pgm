// Multi-tenant SaaS cluster with shared platform and strict isolation
// Mermaid mapping: complex architecture graph with tenant groups and shared services
diagram architecture {
    title: "K8s 多租户隔离拓扑"

    group edge "统一入口" {
        entity[external] global_dns "Global DNS"
        entity[gateway] api_gateway "Tenant API Gateway"
        entity[gateway] auth_proxy "Auth Proxy"
    }

    group tenant_a "tenant-a" {
        entity[frontend] a_web "tenant-a-web x4"
        entity[service] a_api "tenant-a-api x8"
        entity[service] a_worker "tenant-a-worker x12"
        entity[cache] a_cache "tenant-a-redis"
    }

    group tenant_b "tenant-b" {
        entity[frontend] b_web "tenant-b-web x4"
        entity[service] b_api "tenant-b-api x8"
        entity[service] b_worker "tenant-b-worker x12"
        entity[cache] b_cache "tenant-b-redis"
    }

    group shared_platform "共享平台" {
        entity[service] tenant_ctrl "Tenant Control Plane"
        entity[service] policy "Policy Engine"
        entity[service] secrets "Secrets Manager"
        entity[service] billing "Billing Service"
        entity[service] audit "Audit Log Service"
    }

    group shared_data "共享数据层" {
        entity[database] postgres "Shared PostgreSQL"
        entity[queue] kafka "Shared Kafka"
        entity[storage] object_store "Object Storage"
    }

    global_dns -> api_gateway
    api_gateway -> auth_proxy
    auth_proxy -> a_web "tenant-a route"
    auth_proxy -> b_web "tenant-b route"

    a_web -> a_api
    a_api -> a_worker "async jobs"
    a_api -> a_cache
    a_api -> postgres
    a_worker -> kafka
    a_worker -> object_store

    b_web -> b_api
    b_api -> b_worker "async jobs"
    b_api -> b_cache
    b_api -> postgres
    b_worker -> kafka
    b_worker -> object_store

    tenant_ctrl -> a_api "provision"
    tenant_ctrl -> b_api "provision"
    policy -> a_api "enforce quota"
    policy -> b_api "enforce quota"
    secrets -> a_api
    secrets -> b_api
    billing -> postgres
    audit -> kafka
    a_api -> audit "tenant events"
    b_api -> audit "tenant events"
}
