// 最简架构图：单向数据流
// Mermaid 对照: graph TD; User-->App-->DB
diagram architecture {
    title: "单向数据流"

    entity[frontend] user "用户" {
        semantic: user
    }
    entity[service] app "应用"
    entity[database] db "数据存储" {
        semantic: postgres
    }

    user -> app
    app -> db
}
