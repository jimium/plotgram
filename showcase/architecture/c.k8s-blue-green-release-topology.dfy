// Blue-green deployment topology for a large production service
// Mermaid mapping: complex architecture graph for release traffic switching
diagram architecture {
    title: "K8s 蓝绿发布拓扑"

    group edge "入口流量" {
        entity dns "DNS" { type: external }
        entity cdn "CDN" { type: external }
        entity waf "WAF" { type: gateway }
        entity ingress "Ingress Gateway" { type: gateway }
    }

    group blue_env "Blue 生产环境" {
        entity blue_gateway "gateway-blue x8" { type: service }
        entity blue_checkout "checkout-blue x12" { type: service }
        entity blue_payment "payment-blue x10" { type: service }
        entity blue_worker "worker-blue x16" { type: service }
    }

    group green_env "Green 候选环境" {
        entity green_gateway "gateway-green x8" { type: service }
        entity green_checkout "checkout-green x12" { type: service }
        entity green_payment "payment-green x10" { type: service }
        entity green_worker "worker-green x16" { type: service }
    }

    group control_plane "发布控制面" {
        entity argo_rollouts "Argo Rollouts" { type: service }
        entity metrics "Prometheus" { type: service }
        entity logs "Loki" { type: service }
        entity feature_flag "Feature Flag Service" { type: service }
    }

    group shared_backend "共享后端" {
        entity postgres "PostgreSQL" { type: database }
        entity redis "Redis" { type: cache }
        entity kafka "Kafka" { type: queue }
    }

    dns -> cdn
    cdn -> waf
    waf -> ingress
    ingress -> blue_gateway "90% traffic"
    ingress -> green_gateway "10% traffic"

    blue_gateway -> blue_checkout
    blue_checkout -> blue_payment
    blue_checkout -> blue_worker "async events"

    green_gateway -> green_checkout
    green_checkout -> green_payment
    green_checkout -> green_worker "async events"

    blue_checkout -> postgres
    blue_payment -> postgres
    blue_gateway -> redis
    blue_worker -> kafka

    green_checkout -> postgres
    green_payment -> postgres
    green_gateway -> redis
    green_worker -> kafka

    argo_rollouts -> ingress "shift traffic"
    argo_rollouts -> green_gateway "promote"
    argo_rollouts -> blue_gateway "scale down"
    green_checkout -> metrics
    green_payment -> metrics
    green_checkout -> logs
    green_payment -> logs
    metrics -> argo_rollouts "health signal"
    logs -> argo_rollouts "error signal"
    feature_flag -> green_checkout
    feature_flag -> blue_checkout
}
