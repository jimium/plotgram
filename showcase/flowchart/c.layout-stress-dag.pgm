// Layout Stress: Complex DAG & Routing
// 算法测试场景：测试 Sugiyama/DAG 布局引擎对复杂回环（Back-edges）、自环（Self-loops）、跨多层长边（Cross-layer edges）的路由与避障处理能力。
// 拓扑特征：包含 11 个节点，多条反馈边、快速通道边，以及自环测试节点重叠与边交叉情况。
diagram flowchart {
    title: "布局测试：复杂有向无环图与长边路由"
    config {
        direction: top-to-bottom
    }

    entity[start] start "启动"
    entity[process] n1 "节点 1"
    entity[decision] n2 "节点 2"
    entity[process] n3 "节点 3"
    entity[decision] n4 "节点 4"
    entity[decision] n5 "节点 5"
    entity[process] n6 "节点 6"
    entity[process] n7 "节点 7"
    entity[decision] n8 "节点 8"
    entity[decision] n9 "节点 9"
    entity[end] end "结束"

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
