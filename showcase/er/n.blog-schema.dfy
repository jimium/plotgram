// 博客系统 ER 模型
// Mermaid 对照: erDiagram 多实体关系
diagram er {
    title: "博客系统数据模型"

    entity[database] user "User" {
        meta.pk: "id"
        meta.fields: "username\nemail"
    }
    entity[database] post "Post" {
        meta.pk: "id"
        meta.fk: "user_id"
        meta.fields: "title\nbody"
    }
    entity[database] comment "Comment" {
        meta.pk: "id"
        meta.fk: "post_id"
        meta.fields: "content"
    }
    entity[database] tag "Tag" {
        meta.pk: "id"
        meta.fields: "name"
    }
    entity[database] post_tag "PostTag" {
        meta.pk: "id"
        meta.fields: "fk.post_id\nfk.tag_id"
    }

    user -> post "发表" { cardinality: "1:N" }
    user -> comment "评论" { cardinality: "1:N" }
    post -> comment "包含" { cardinality: "1:N" }
    post -> post_tag "标记" { cardinality: "1:N" }
    tag -> post_tag "被引用" { cardinality: "1:N" }
}
