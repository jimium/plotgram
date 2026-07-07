// Community interaction data model
// Business Scenario: Social interaction, showing core relationship model among users, posts, comments, likes, and follows
// Mermaid Mapping: erDiagram with multi-entity relationships and cardinality
diagram er {
    title: "社交互动核心数据模型"

    entity[database] user "User (用户)" {
        meta.pk: "id"
        meta.fields: "username\navatar\nbio"
    }
    entity[database] follow "Follow (关注关系)" {
        meta.pk: "id"
        meta.fields: "fk.follower_id\nfk.following_id\ncreated_at"
    }
    entity[database] post "Post (动态)" {
        meta.pk: "id"
        meta.fk: "user_id"
        meta.fields: "content\nmedia_urls"
    }
    entity[database] comment "Comment (评论)" {
        meta.pk: "id"
        meta.fk: "post_id\nuser_id"
        meta.fields: "content\nparent_id"
    }
    entity[database] like "Like (点赞)" {
        meta.pk: "id"
        meta.fields: "fk.user_id\nfk.target_id\ntarget_type"
    }

    user -> follow "关注" { cardinality: "1:N" }
    user -> follow "被关注" { cardinality: "1:N" }
    user -> post "发布" { cardinality: "1:N" }
    user -> comment "发表" { cardinality: "1:N" }
    user -> like "点赞操作" { cardinality: "1:N" }
    post -> comment "包含" { cardinality: "1:N" }
    post -> like "被点赞" { cardinality: "1:N" }
    comment -> like "被点赞" { cardinality: "1:N" }
}
