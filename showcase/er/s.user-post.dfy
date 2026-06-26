// 两实体一对多关系
// Mermaid 对照: erDiagram; USER ||--o{ POST : writes
diagram er {
    title: "用户与文章"
    config {
        direction: left-to-right
    }

    entity[database] user "User" {
        meta.pk: "id"
        meta.fields: "username\nemail"
    }
    entity[database] post "Post" {
        meta.pk: "id"
        meta.fk: "user_id"
        meta.fields: "title\ncontent"
    }

    user -> post "发表" {
        cardinality: "1:N"
    }
}
