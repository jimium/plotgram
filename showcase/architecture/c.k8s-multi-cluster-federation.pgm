// Multi-cluster production topology across regions
// Mermaid mapping: complex architecture graph with regional groups
diagram architecture {
    title: "K8s 多集群跨地域拓扑"

    group global_edge "全球接入" {
        entity[external] global_dns "Global DNS"
        entity[gateway] traffic_mgr "Traffic Manager"
        entity[gateway] edge_waf "Edge WAF"
    }

    group cn_east "华东主集群" {
        entity[gateway] east_ingress "Ingress East"
        entity[service] east_gateway "api-gateway x8"
        entity[service] east_order "order-core x14"
        entity[service] east_payment "payment-core x12"
        entity[service] east_user "user-core x10"
    }

    group cn_north "华北灾备集群" {
        entity[gateway] north_ingress "Ingress North"
        entity[service] north_gateway "api-gateway x4"
        entity[service] north_order "order-core x8"
        entity[service] north_payment "payment-core x6"
        entity[service] north_user "user-core x6"
    }

    group global_platform "全局平台能力" {
        entity[service] mesh_cp "Service Mesh Control Plane"
        entity[service] gitops "GitOps Controller"
        entity[storage] registry "Container Registry"
        entity[service] monitor "Unified Monitoring"
    }

    group shared_data "共享数据与消息" {
        entity[database] mysql_primary "MySQL Primary"
        entity[database] mysql_replica "MySQL Replica"
        entity[cache] redis_global "Redis Global Cache"
        entity[queue] mq_global "Global Event Bus"
    }

    global_dns -> traffic_mgr
    traffic_mgr -> edge_waf
    edge_waf -> east_ingress "primary traffic"
    edge_waf -> north_ingress "failover traffic"

    east_ingress -> east_gateway
    east_gateway -> east_order
    east_gateway -> east_payment
    east_gateway -> east_user

    north_ingress -> north_gateway
    north_gateway -> north_order
    north_gateway -> north_payment
    north_gateway -> north_user

    east_order -> mysql_primary
    east_payment -> mysql_primary
    east_user -> redis_global
    east_order -> mq_global

    north_order -> mysql_replica
    north_payment -> mysql_replica
    north_user -> redis_global
    north_order -> mq_global

    mesh_cp -> east_gateway
    mesh_cp -> east_order
    mesh_cp -> north_gateway
    mesh_cp -> north_order
    gitops -> east_gateway "sync"
    gitops -> north_gateway "sync"
    registry -> east_gateway "image pull"
    registry -> north_gateway "image pull"

    east_gateway -> monitor
    east_order -> monitor
    north_gateway -> monitor
    north_order -> monitor

    mysql_primary --> mysql_replica "replication"
    east_order --> north_order "cross-region replay"
    east_payment --> north_payment "cold standby"
}
