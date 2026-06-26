// Programming learning syllabus
// Business Scenario: Education and learning, demonstrating a systematic learning path and knowledge structure for a programming language (e.g., Rust)
// Mermaid Mapping: mindmap for hierarchical knowledge tree expansion
diagram mindmap {
    title: "Rust 编程学习大纲"

    entity root "Rust 语言" { type: root }

    entity basic "基础语法" { type: main }
    entity vars "变量与可变性" { type: branch }
    entity types "数据类型" { type: leaf }
    entity control "控制流" { type: leaf }

    entity ownership "所有权系统" { type: main }
    entity borrow "所有权与借用" { type: branch }
    entity lifetime "生命周期" { type: leaf }

    entity advanced "高级特性" { type: main }
    entity generic "泛型与 Traits" { type: branch }
    entity error "错误处理" { type: leaf }
    entity macro "宏 (Macros)" { type: leaf }

    entity concurrency "并发编程" { type: main }
    entity threads "线程与消息传递" { type: branch }
    entity async "Async/Await" { type: leaf }

    root -> basic
    basic -> vars
    basic -> types
    basic -> control
    
    root -> ownership
    ownership -> borrow
    ownership -> lifetime
    
    root -> advanced
    advanced -> generic
    advanced -> error
    advanced -> macro
    
    root -> concurrency
    concurrency -> threads
    concurrency -> async
}
