// Layout Stress: Dense Graph Routing
// 算法测试场景：测试 ER 图等使用 Force-directed 或 Orthogonal 算法时，处理高密度、多对多、类完全图（Clique）连接的能力。
// 拓扑特征：高密度的交叉连接网络，用于验证算法的引力斥力计算稳定性、边缘交叉最小化与重叠处理。
diagram er {
    title: "布局测试：高密度网状图连线路由"

    entity[database] order "订单表 (Order)"
    entity[database] user "用户表 (User)"
    entity[database] product "商品表 (Product)"
    entity[database] store "店铺表 (Store)"
    entity[database] coupon "优惠券表 (Coupon)"
    entity[database] payment "支付表 (Payment)"
    entity[database] log "日志表 (Log)"

    // User 的高频连出
    user -> order "创建"
    user -> coupon "领取"
    user -> payment "支付"
    user -> store "关注"
    user -> log "操作日志"
    
    // Store 的连出与交互
    store -> product "上架"
    store -> coupon "发行"
    store -> order "履约"
    store -> log "商家日志"
    
    // Product 的复杂关联
    product -> order "包含于"
    product -> coupon "适用范围"
    
    // Order 关联其他业务单据
    order -> payment "生成支付单"
    order -> coupon "核销"
    order -> log "订单状态变更"
    
    // Payment 与其他实体的弱关联
    payment -> log "流水记录"
    payment -> coupon "组合支付抵扣"
}
