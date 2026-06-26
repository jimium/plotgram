// Layout Stress: Complex DAG & Routing
// 算法测试场景：测试 Sugiyama/DAG 布局引擎对复杂回环（Back-edges）、自环（Self-loops）、跨多层长边（Cross-layer edges）的路由与避障处理能力。
// 拓扑特征：包含 11 个节点，多条反馈边、快速通道边，以及自环测试节点重叠与边交叉情况。
diagram flowchart {
    title: "布局测试：复杂有向无环图与长边路由"
    config {
        direction: top-to-bottom
    }

    entity start "启动" { type: start }
    entity n1 "节点 1" { type: process }
    entity n2 "节点 2" { type: decision }
    entity n3 "节点 3" { type: process }
    entity n4 "节点 4" { type: decision }
    entity n5 "节点 5" { type: decision }
    entity n6 "节点 6" { type: process }
    entity n7 "节点 7" { type: process }
    entity n8 "节点 8" { type: decision }
    entity n9 "节点 9" { type: decision }
    entity end "结束" { type: end }

    start -> n1
    start -> n2 "旁路"
    n1 -> n2
    n2 -> n3 "条件 A"
    n2 -> n4 "条件 B"
    n3 -> n5
    n4 -> n5
    n5 -> n6 "通过"
    n5 -> n2 "重试 (回环边)"
    n6 -> n7
    n7 -> n8
    n8 -> n9 "确认"
    n8 -> n5 "打回 (回环边)"
    n9 -> end
    n9 -> n9 "等待异步 (自环)"
    
    // 测试跨层长边
    n1 -> end "快速通道 (跨多层)"
    n3 -> n7 "跳层边"
    
    // 测试节点自环
    n4 -> n4 "自检"
}
