// Node health lifecycle under pressure and recovery
// Mermaid mapping: complex state diagram with operational branches
diagram state {
    title: "K8s 节点压力与恢复状态机"

    entity init "初始化" { type: initial }
    entity ready "Ready" { type: state }
    entity memory_pressure "MemoryPressure" { type: state }
    entity disk_pressure "DiskPressure" { type: state }
    entity not_ready "NotReady" { type: state }
    entity cordoned "Cordoned" { type: state }
    entity draining "Draining" { type: state }
    entity recovering "Recovering" { type: state }
    entity replaced "Replaced" { type: final }
    entity healthy "Healthy" { type: final }
    entity decide "Can Recover?" { type: choice }

    init -> ready
    ready -> memory_pressure "memory threshold exceeded"
    ready -> disk_pressure "disk threshold exceeded"
    memory_pressure -> not_ready "evictions fail"
    disk_pressure -> not_ready "image gc ineffective"
    memory_pressure -> ready "load reduced"
    disk_pressure -> ready "space reclaimed"
    not_ready -> cordoned "mark unschedulable"
    cordoned -> draining "evict workloads"
    draining -> decide
    decide -> recovering "node repaired"
    decide -> replaced "node replaced"
    recovering -> ready "kubelet rejoins"
    ready -> healthy "stable for observation window"
}
