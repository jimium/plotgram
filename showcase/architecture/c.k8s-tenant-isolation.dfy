// Multi-tenant SaaS cluster with shared platform and strict isolation
// Mermaid mapping: complex architecture graph with tenant groups and shared services
diagram architecture {
    title: "K8s 多租户隔离拓扑"

    group edge "统一入口" {
        entity global_dns "Global DNS" { type: external }
        entity api_gateway "Tenant API Gateway" { type: gateway }
        entity auth_proxy "Auth Proxy" { type: gateway }
    }

    group tenant_a "tenant-a" {
        entity a_web "tenant-a-web x4" { type: frontend }
        entity a_api "tenant-a-api x8" { type: service }
        entity a_worker "tenant-a-worker x12" { type: service }
        entity a_cache "tenant-a-redis" { type: cache }
    }

    group tenant_b "tenant-b" {
        entity b_web "tenant-b-web x4" { type: frontend }
        entity b_api "tenant-b-api x8" { type: service }
        entity b_worker "tenant-b-worker x12" { type: service }
        entity b_cache "tenant-b-redis" { type: cache }
    }

    group shared_platform "共享平台" {
        entity tenant_ctrl "Tenant Control Plane" { type: service }
        entity policy "Policy Engine" { type: service }
        entity secrets "Secrets Manager" { type: service }
        entity billing "Billing Service" { type: service }
        entity audit "Audit Log Service" { type: service }
    }

    group shared_data "共享数据层" {
        entity postgres "Shared PostgreSQL" { type: database }
        entity kafka "Shared Kafka" { type: queue }
        entity object_store "Object Storage" { type: storage }
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
