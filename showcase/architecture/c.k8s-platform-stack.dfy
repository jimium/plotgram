// Platform team view of a large K8s cluster stack
// Mermaid mapping: layered architecture with infra and workload groups
diagram architecture {
    title: "K8s 平台栈与业务工作负载"

    group client_layer "入口与访问" {
        entity mobile "Mobile App" { type: frontend }
        entity web "Web Portal" { type: frontend }
        entity api_gateway "API Gateway" { type: gateway }

        mobile -> api_gateway
        web -> api_gateway
    }

    group workload_layer "业务命名空间" {
        group commerce_ns "commerce" {
            entity cart_svc "cart-service x8" { type: service }
            entity checkout_svc "checkout-service x12" { type: service }
            entity pricing_svc "pricing-service x6" { type: service }

            cart_svc -> pricing_svc
            checkout_svc -> pricing_svc
        }

        group ops_ns "ops" {
            entity ops_console "ops-console x3" { type: frontend }
            entity report_svc "report-service x5" { type: service }
            entity audit_svc "audit-service x4" { type: service }
        }

        // 跨子 group 的边：写在父 group 内
        checkout_svc -> audit_svc
    }

    group platform_layer "平台能力" {
        entity ingress_nginx "Ingress NGINX" { type: gateway }
        entity cert_manager "cert-manager" { type: service }
        entity cni "CNI Plugin" { type: service }
        entity csi "CSI Driver" { type: service }
        entity external_dns "external-dns" { type: service }
        entity argo_rollouts "Argo Rollouts" { type: service }

        cert_manager -> ingress_nginx
        external_dns -> ingress_nginx
    }

    group obs_layer "可观测与运维" {
        entity prometheus "Prometheus" { type: service }
        entity loki "Loki" { type: service }
        entity tempo "Tempo" { type: service }
        entity grafana "Grafana" { type: frontend }

        prometheus --> grafana
        loki --> grafana
        tempo --> grafana
    }

    group data_layer "数据与中间件" {
        entity mysql "MySQL Cluster" { type: database }
        entity redis "Redis" { type: cache }
        entity kafka "Kafka" { type: queue }
        entity minio "MinIO" { type: storage }
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
