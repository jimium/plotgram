// Blue-green deployment topology for a large production service
// Mermaid mapping: complex architecture graph for release traffic switching
diagram architecture {
    title: "K8s 蓝绿发布拓扑"

    group edge "入口流量" {
        entity[external] dns "DNS"
        entity[external] cdn "CDN"
        entity[gateway] waf "WAF"
        entity[gateway] ingress "Ingress Gateway"
    }

    group blue_env "Blue 生产环境" {
        entity[service] blue_gateway "gateway-blue x8"
        entity[service] blue_checkout "checkout-blue x12"
        entity[service] blue_payment "payment-blue x10"
        entity[service] blue_worker "worker-blue x16"
    }

    group green_env "Green 候选环境" {
        entity[service] green_gateway "gateway-green x8"
        entity[service] green_checkout "checkout-green x12"
        entity[service] green_payment "payment-green x10"
        entity[service] green_worker "worker-green x16"
    }

    group control_plane "发布控制面" {
        entity[service] argo_rollouts "Argo Rollouts"
        entity[service] metrics "Prometheus"
        entity[service] logs "Loki"
        entity[service] feature_flag "Feature Flag Service"
    }

    group shared_backend "共享后端" {
        entity[database] postgres "PostgreSQL"
        entity[cache] redis "Redis"
        entity[queue] kafka "Kafka"
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
