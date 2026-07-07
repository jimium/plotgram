// Platform team view of a large K8s cluster stack
// Mermaid mapping: layered architecture with infra and workload groups
diagram architecture {
    title: "K8s 平台栈与业务工作负载"

    group client_layer "入口与访问" {
        entity[frontend] mobile "Mobile App"
        entity[frontend] web "Web Portal"
        entity[gateway] api_gateway "API Gateway"

        mobile -> api_gateway
        web -> api_gateway
    }

    group workload_layer "业务命名空间" {
        group commerce_ns "commerce" {
            entity[service] cart_svc "cart-service x8"
            entity[service] checkout_svc "checkout-service x12"
            entity[service] pricing_svc "pricing-service x6"

            cart_svc -> pricing_svc
            checkout_svc -> pricing_svc
        }

        group ops_ns "ops" {
            entity[frontend] ops_console "ops-console x3"
            entity[service] report_svc "report-service x5"
            entity[service] audit_svc "audit-service x4"
        }

        // 跨子 group 的边：写在父 group 内
        checkout_svc -> audit_svc
    }

    group platform_layer "平台能力" {
        entity[gateway] ingress_nginx "Ingress NGINX"
        entity[service] cert_manager "cert-manager"
        entity[service] cni "CNI Plugin"
        entity[service] csi "CSI Driver"
        entity[service] external_dns "external-dns"
        entity[service] argo_rollouts "Argo Rollouts"

        cert_manager -> ingress_nginx
        external_dns -> ingress_nginx
    }

    group obs_layer "可观测与运维" {
        entity[service] prometheus "Prometheus"
        entity[service] loki "Loki"
        entity[service] tempo "Tempo"
        entity[frontend] grafana "Grafana"

        prometheus --> grafana
        loki --> grafana
        tempo --> grafana
    }

    group data_layer "数据与中间件" {
        entity[database] mysql "MySQL Cluster"
        entity[cache] redis "Redis"
        entity[queue] kafka "Kafka"
        entity[storage] minio "MinIO"
    }

    api_gateway -> ingress_nginx
    ingress_nginx -> cart_svc
    ingress_nginx -> checkout_svc
    ingress_nginx -> ops_console

    report_svc -> kafka
    audit_svc -> kafka

    cart_svc -> redis
    checkout_svc -> mysql
    pricing_svc -> redis
    report_svc -> mysql
    audit_svc -> minio

    cni -> cart_svc
    cni -> checkout_svc
    csi -> mysql
    argo_rollouts -> checkout_svc "canary"
    argo_rollouts -> ops_console "blue-green"

    cart_svc -> prometheus
    checkout_svc -> prometheus
    audit_svc -> loki
    checkout_svc -> tempo
}
