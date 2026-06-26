// 电商平台全栈架构：多层分组 + 外部依赖
// Mermaid 对照: 复杂 subgraph 嵌套架构图
diagram architecture {
    title: "电商平台架构"

    group client_layer "接入层" {
        entity web "Web 商城" { type: frontend }
        entity app "移动 App" { type: frontend }
        entity admin "运营后台" { type: frontend }
    }

    group gateway_layer "网关层" {
        entity cdn "CDN" { type: external }
        entity lb "负载均衡" { type: gateway }
        entity api_gw "API 网关" { type: gateway }

        cdn -> lb
        lb -> api_gw
    }

    group service_layer "业务服务" {
        entity user_svc "用户服务" { type: service }
        entity product_svc "商品服务" { type: service }
        entity order_svc "订单服务" { type: service }
        entity payment_svc "支付服务" { type: service }
        entity search_svc "搜索服务" { type: service }
        entity notify_svc "通知服务" { type: service }

        order_svc -> payment_svc
    }

    group data_layer "数据层" {
        entity mysql "MySQL 集群" {
            type: database
            semantic: mysql
        }
        entity redis "Redis 缓存" {
            type: cache
            semantic: redis
        }
        entity es "Elasticsearch" {
            type: database
            semantic: elasticsearch
        }
        entity mq "RabbitMQ" {
            type: queue
            semantic: rabbitmq
        }
    }

    group external "外部系统" {
        entity alipay "支付宝" { type: external }
        entity sms "短信网关" { type: external }
    }

    web -> cdn
    app -> lb
    admin -> api_gw
    api_gw -> user_svc
    api_gw -> product_svc
    api_gw -> order_svc
    api_gw -> search_svc
    payment_svc -> alipay
    user_svc -> mysql
    product_svc -> mysql
    order_svc -> mysql
    order_svc -> redis
    search_svc -> es
    order_svc -> mq "订单事件"
    mq --> notify_svc "异步通知"
    notify_svc -> sms
}
