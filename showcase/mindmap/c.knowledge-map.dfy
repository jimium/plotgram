// 知识图谱：AI/ML 领域概念
// Mermaid 对照: 复杂 mindmap 知识树
diagram mindmap {
    title: "AI/ML 知识图谱"

    entity ai "人工智能" { type: root }

    entity ml "机器学习" { type: main }
    entity supervised "监督学习" { type: leaf }
    entity unsupervised "无监督学习" { type: leaf }
    entity reinforcement "强化学习" { type: leaf }

    entity dl "深度学习" { type: main }
    entity cnn "CNN" { type: leaf }
    entity rnn "RNN" { type: leaf }
    entity transformer "Transformer" { type: branch }

    entity llm "大语言模型" { type: branch }
    entity pretrain "预训练" { type: leaf }
    entity finetune "微调" { type: leaf }
    entity rag "RAG" { type: leaf }
    entity agent "Agent" { type: leaf }

    entity infra "基础设施" { type: main }
    entity gpu "GPU 集群" { type: leaf }
    entity vector_db "向量数据库" { type: leaf }

    ai -> ml
    ml -> supervised
    ml -> unsupervised
    ml -> reinforcement
    ai -> dl
    dl -> cnn
    dl -> rnn
    dl -> transformer
    transformer -> llm
    llm -> pretrain
    llm -> finetune
    llm -> rag
    llm -> agent
    ai -> infra
    infra -> gpu
    infra -> vector_db
}
