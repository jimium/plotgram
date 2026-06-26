// Layout Stress: Complex State Transitions
// 算法测试场景：测试状态图引擎处理包含多重循环、自发转换（Self-transitions）、以及交错分支回路的布局表现。
// 拓扑特征：高度闭环的有限状态机（FSM），用于验证状态节点的相对位置和连线弧度的美观性。
diagram state {
    title: "布局测试：复杂状态循环与多重回环"

    entity init "初始化" { type: initial }
    entity pending "等待处理" { type: state }
    entity locked "资源锁定" { type: state }
    entity processing "处理中" { type: state }
    entity success "处理成功" { type: state }
    entity fail "处理失败" { type: state }
    entity refunding "退款中" { type: state }
    entity check "超时检查" { type: choice }
    entity done "结束" { type: final }

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
