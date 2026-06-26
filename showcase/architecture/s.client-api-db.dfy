// 最简架构图：单向数据流
// Mermaid 对照: graph TD; User-->App-->DB
diagram architecture {
    title: "单向数据流"

    entity user "用户" {
        type: frontend
        semantic: user
    }
    entity app "应用" { type: service }
    entity db "数据存储" {
        type: database
        semantic: postgres
    }

    user -> app
    app -> db
}
