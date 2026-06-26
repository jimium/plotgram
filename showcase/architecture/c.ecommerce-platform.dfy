// 电商平台全栈架构：多层分组 + 外部依赖
// Mermaid 对照: 复杂 subgraph 嵌套架构图
diagram architecture {
    title: "电商平台架构"

    group client_layer "接入层" {
        entity[frontend] web "Web 商城"
        entity[frontend] app "移动 App"
        entity[frontend] admin "运营后台"
    }

    group gateway_layer "网关层" {
        entity[external] cdn "CDN"
        entity[gateway] lb "负载均衡"
        entity[gateway] api_gw "API 网关"

        cdn -> lb
        lb -> api_gw
    }

    group service_layer "业务服务" {
        entity[service] user_svc "用户服务"
        entity[service] product_svc "商品服务"
        entity[service] order_svc "订单服务"
        entity[service] payment_svc "支付服务"
        entity[service] search_svc "搜索服务"
        entity[service] notify_svc "通知服务"

        order_svc -> payment_svc
    }

    group data_layer "数据层" {
        entity[database] mysql "MySQL 集群" {
            semantic: mysql
        }
        entity[cache] redis "Redis 缓存" {
            semantic: redis
        }
        entity[database] es "Elasticsearch" {
            semantic: elasticsearch
        }
        entity[queue] mq "RabbitMQ" {
            semantic: rabbitmq
        }
    }

    group external "外部系统" {
        entity[external] alipay "支付宝"
        entity[external] sms "短信网关"
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
