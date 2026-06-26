// Node health lifecycle under pressure and recovery
// Mermaid mapping: complex state diagram with operational branches
diagram state {
    title: "K8s 节点压力与恢复状态机"

    entity[initial] init "初始化"
    entity[state] ready "Ready"
    entity[state] memory_pressure "MemoryPressure"
    entity[state] disk_pressure "DiskPressure"
    entity[state] not_ready "NotReady"
    entity[state] cordoned "Cordoned"
    entity[state] draining "Draining"
    entity[state] recovering "Recovering"
    entity[final] replaced "Replaced"
    entity[final] healthy "Healthy"
    entity[choice] decide "Can Recover?"

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
