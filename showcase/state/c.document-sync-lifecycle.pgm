// Documentation synchronization lifecycle from stale content to published update
// Mermaid mapping: complex state diagram for docops automation
diagram state {
    title: "文档同步生命周期"

    entity[initial] init "初始化"
    entity[state] in_sync "InSync"
    entity[state] stale "Stale"
    entity[state] analyzing "Analyzing"
    entity[state] drafting "Drafting"
    entity[state] reviewing "Reviewing"
    entity[final] published "Published"
    entity[state] rejected "Rejected"
    entity[final] failed "Failed"
    entity[choice] gate "评审通过？"

    init -> in_sync
    in_sync -> stale "source changed"
    stale -> analyzing "trigger sync job"
    analyzing -> drafting "context collected"
    drafting -> reviewing "draft generated"
    reviewing -> gate
    gate -> published "yes"
    gate -> rejected "no"
    rejected -> drafting "apply feedback"
    analyzing -> failed "context unavailable"
}
