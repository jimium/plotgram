// 云原生技术栈：K8s + 可观测性
// Mermaid 对照: 复杂 C4/架构图
diagram architecture {
    title: "云原生技术栈"

    group ingress "入口" {
        entity ingress_ctrl "Ingress Controller" {
            type: gateway
            semantic: ingress
        }
        entity cert_mgr "证书管理" {
            type: service
            semantic: cert_manager
        }
    }

    group k8s "Kubernetes 集群" {
        group apps "应用 Pod" {
            entity api "API 服务" {
                type: service
                status: healthy
            }
            entity worker "后台 Worker" { type: service }
        }
        group platform "平台组件" {
            entity config_center "配置中心" { type: service }
            entity discovery "服务发现" { type: service }
        }
    }

    group observability "可观测性" {
        entity metrics "Prometheus" {
            type: service
            semantic: prometheus
        }
        entity logs "Loki" {
            type: service
            semantic: loki
        }
        entity traces "Jaeger" {
            type: service
            semantic: jaeger
        }
        entity grafana "Grafana" {
            type: frontend
            semantic: grafana
        }
    }

    group data "持久化" {
        entity pg "PostgreSQL" {
            type: database
            semantic: postgres
        }
        entity s3 "对象存储" {
            type: storage
            semantic: s3
        }
    }

    ingress_ctrl -> api
    cert_mgr -> ingress_ctrl
    api -> discovery
    api -> config_center
    worker -> config_center
    api -> pg
    worker -> s3
    api -> metrics
    worker -> metrics
    api -> logs
    api -> traces
    metrics --> grafana
    logs --> grafana
    traces --> grafana
}
