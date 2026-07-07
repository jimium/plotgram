// Layout Stress: Complex State Transitions
// 算法测试场景：测试状态图引擎处理包含多重循环、自发转换（Self-transitions）、以及交错分支回路的布局表现。
// 拓扑特征：高度闭环的有限状态机（FSM），用于验证状态节点的相对位置和连线弧度的美观性。
diagram state {
    title: "布局测试：复杂状态循环与多重回环"

    entity[initial] init "初始化"
    entity[state] pending "等待处理"
    entity[state] locked "资源锁定"
    entity[state] processing "处理中"
    entity[state] success "处理成功"
    entity[state] fail "处理失败"
    entity[state] refunding "退款中"
    entity[choice] check "超时检查"
    entity[final] done "结束"

    init -> pending
    pending -> locked
    locked -> processing
    processing -> success
    success -> done
    
    // 异常与回环处理
    processing -> fail "业务异常"
    fail -> refunding
    refunding -> done
    
    // 状态分支与多重回环
    locked -> check "发生超时"
    check -> refunding "达到最大重试 (退出)"
    check -> pending "允许重试 (大回环)"
    
    // 节点自环测试
    pending -> pending "等待外部事件唤醒 (自环)"
    locked -> locked "重试获取锁 (自环)"
    processing -> processing "进度更新 (自环)"
}
