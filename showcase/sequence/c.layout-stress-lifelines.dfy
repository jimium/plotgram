// Layout Stress: Dense Sequence Messages
// 算法测试场景：测试时序图在面对密集消息、自调用（Self-messages）、异步跨度长回调时的生命线渲染与消息排布能力。
// 拓扑特征：长生命周期、穿插的回调边、重叠的时序逻辑。
diagram sequence {
    title: "布局测试：密集时序消息与长跨度回调"

    entity client "移动端" { type: actor }
    entity gateway "API 网关" { type: boundary }
    entity svc_a "微服务 A" { type: control }
    entity svc_b "微服务 B" { type: control }
    entity db "分布式数据库" { type: database }

    client -> gateway "1. 发起同步请求"
    gateway -> svc_a "2. 路由转发"
    
    // 自调用与密集 DB 交互
    svc_a -> svc_a "3. 内部参数校验 (Self-call)"
    svc_a -> db "4. 查询权限数据"
    db --> svc_a "5. 返回权限结果"
    
    // 异步任务分发
    svc_a -> svc_b "6. 投递异步计算任务"
    svc_a --> gateway "7. 快速返回 Ack"
    gateway --> client "8. 202 Accepted (客户端进入轮询)"
    
    // 独立微服务 B 的处理链路
    svc_b -> svc_b "9. 资源初始化 (Self-call)"
    svc_b -> db "10. 写入计算中间态"
    db --> svc_b "11. 写入成功"
    
    // 长跨度回调与推送
    svc_b -> svc_a "12. 任务完成回调通知 (长跨度边)"
    svc_a -> client "13. WebSocket 推送最终结果 (跨越多条生命线)"
    
    client -> client "14. 界面渲染更新"
}
