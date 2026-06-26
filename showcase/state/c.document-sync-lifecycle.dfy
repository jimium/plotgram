// Documentation synchronization lifecycle from stale content to published update
// Mermaid mapping: complex state diagram for docops automation
diagram state {
    title: "文档同步生命周期"

    entity init "初始化" { type: initial }
    entity in_sync "InSync" { type: state }
    entity stale "Stale" { type: state }
    entity analyzing "Analyzing" { type: state }
    entity drafting "Drafting" { type: state }
    entity reviewing "Reviewing" { type: state }
    entity published "Published" { type: final }
    entity rejected "Rejected" { type: state }
    entity failed "Failed" { type: final }
    entity gate "评审通过？" { type: choice }

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
