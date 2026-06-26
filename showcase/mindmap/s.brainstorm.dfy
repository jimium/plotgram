// 头脑风暴：中心主题 + 三个分支
// Mermaid 对照: mindmap; root((主题)); 分支1; 分支2; 分支3
diagram mindmap {
    title: "头脑风暴"

    entity root "产品规划" { type: root }
    entity feature "功能需求" { type: main }
    entity tech "技术方案" { type: main }
    entity market "市场调研" { type: main }

    root -> feature
    root -> tech
    root -> market
}
