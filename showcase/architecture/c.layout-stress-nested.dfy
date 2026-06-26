// Layout Stress: Nested Groups & Cross-Routing
// 算法测试场景：测试架构图（通常使用 Orthogonal 或层次布局）对深度嵌套分组、同级组间互联、跨层级分组的边线路由能力。
// 拓扑特征：3层深度嵌套，多条跨出/跨入边界的连接线，验证边界框计算与避障。
diagram architecture {
    title: "布局测试：深度嵌套分组与跨组路由"

    group external "外部网络" {
        entity client "客户端终端" { type: frontend }
        entity third_party "第三方服务" { type: external }
    }

    group cloud "云端基础设施" {
        group public_subnet "公有子网" {
            entity gateway "API 网关" { type: gateway }
            entity lb "负载均衡器" { type: service }
        }
        
        group private_subnet "私有子网" {
            entity auth_svc "认证微服务" { type: service }
            entity biz_svc "核心业务服务" { type: service }
            entity async_worker "异步任务节点" { type: service }
        }
        
        group data_subnet "数据子网" {
            entity db_master "主数据库" { type: database }
            entity db_replica "只读副本" { type: database }
            entity redis "Redis 集群" { type: cache }
            entity mq "消息队列 Kafka" { type: queue }
        }
    }

    // 常规层级流转
    client -> gateway "HTTPS"
    gateway -> lb
    lb -> auth_svc
    lb -> biz_svc
    
    // 同组内交互
    auth_svc -> redis "读写 Token"
    biz_svc -> db_master "写数据"
    biz_svc -> db_replica "读数据"
    db_master -> db_replica "主从同步"
    
    // 跨组与反向路由
    biz_svc -> mq "发送事件"
    mq -> async_worker "消费事件 (跨组回流)"
    async_worker -> db_master "批量写入"
    
    // 跨越多个边界的长边路由
    biz_svc -> third_party "Webhook 通知 (穿透云端与VPC)"
    client -> third_party "直接调用 (外部互联)"
}
