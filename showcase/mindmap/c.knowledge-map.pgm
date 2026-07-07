// 知识图谱：AI/ML 领域概念
// Mermaid 对照: 复杂 mindmap 知识树
diagram mindmap {
    title: "AI/ML 知识图谱"

    entity[root] ai "人工智能"

    entity[main] ml "机器学习"
    entity[leaf] supervised "监督学习"
    entity[leaf] unsupervised "无监督学习"
    entity[leaf] reinforcement "强化学习"

    entity[main] dl "深度学习"
    entity[leaf] cnn "CNN"
    entity[leaf] rnn "RNN"
    entity[branch] transformer "Transformer"

    entity[branch] llm "大语言模型"
    entity[leaf] pretrain "预训练"
    entity[leaf] finetune "微调"
    entity[leaf] rag "RAG"
    entity[leaf] agent "Agent"

    entity[main] infra "基础设施"
    entity[leaf] gpu "GPU 集群"
    entity[leaf] vector_db "向量数据库"

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
