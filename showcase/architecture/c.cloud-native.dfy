// 云原生技术栈：K8s + 可观测性
// Mermaid 对照: 复杂 C4/架构图
diagram architecture {
    title: "云原生技术栈"

    group ingress "入口" {
        entity[gateway] ingress_ctrl "Ingress Controller" {
            semantic: ingress
        }
        entity[service] cert_mgr "证书管理" {
            semantic: cert_manager
        }
    }

    group k8s "Kubernetes 集群" {
        group apps "应用 Pod" {
            entity[service] api "API 服务" {
                status: healthy
            }
            entity[service] worker "后台 Worker"
        }
        group platform "平台组件" {
            entity[service] config_center "配置中心"
            entity[service] discovery "服务发现"
        }
    }

    group observability "可观测性" {
        entity[service] metrics "Prometheus" {
            semantic: prometheus
        }
        entity[service] logs "Loki" {
            semantic: loki
        }
        entity[service] traces "Jaeger" {
            semantic: jaeger
        }
        entity[frontend] grafana "Grafana" {
            semantic: grafana
        }
    }

    group data "持久化" {
        entity[database] pg "PostgreSQL" {
            semantic: postgres
        }
        entity[storage] s3 "对象存储" {
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
