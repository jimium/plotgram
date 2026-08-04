// Inline demo fixtures (same shapes the T1 acceptance tests exercise).
// NOTE: `build_debug_trace` dispatches on the exact name `hierarchical`
// (registry aliasing is not accepted at the orchestration layer), so every
// fixture here uses `layout: hierarchical` explicitly.

export const FIXTURES = [
  {
    name: "环 + 长边 + 嵌套组 + 自环（TB）",
    source: `diagram {
    profile: flowchart,
    layout: hierarchical

    group outer {
        label: "Outer"
        group inner {
            label: "Inner"
            node a { label: "A" }
            node b { label: "B" }
        }
        node c { label: "C" }
    }
    node d { label: "D" }

    a -> b
    b -> c
    c -> d
    a -> d
    d -> a
    b -> b
}
`,
  },
  {
    name: "微服务分层（组 + 长边）",
    source: `diagram {
    profile: flowchart,
    layout: hierarchical

    group frontend {
        label: "Frontend"
        node web { label: "Web" }
        node mobile { label: "Mobile" }
    }
    group backend {
        label: "Backend"
        node gateway { label: "Gateway" }
        node order_svc { label: "Order" }
        node user_svc { label: "User" }
    }
    node mq { label: "MQ" }
    node db { label: "DB" }

    web -> gateway
    mobile -> gateway
    gateway -> user_svc
    gateway -> order_svc
    user_svc -> db
    order_svc -> db
    order_svc -> mq
    mq -> user_svc
}
`,
  },
  {
    name: "重试自环（LR 方向）",
    source: `diagram {
    profile: flowchart,
    layout: hierarchical { direction: left-to-right }

    node init { label: "Init" }
    node fetch { label: "Fetch" }
    node check { label: "Check" }
    node save { label: "Save" }

    init -> fetch
    fetch -> check
    check -> fetch
    fetch -> fetch
    check -> save
}
`,
  },
];
