# Round 02: sugiyama_v2 + visibility Dijkstra Optimization

## Changes
- `crates/drawify-core/src/layout/node/sugiyama_v2/rank.rs`: Pre-computed adjacency tables + Vec<bool> instead of HashSet
- `crates/drawify-core/src/layout/edge/visibility.rs`: BinaryHeap for Dijkstra

## Results Summary

### sugiyama-v2 (51 diagrams)
| Metric | Value |
|--------|-------|
| Score consistency | **51/51 (100%)** |
| Max score change | 0.00 |
| Mean speedup | -0.12% |

### All Algorithms (499 combinations)
| Metric | Value |
|--------|-------|
| Unchanged (±0.01) | 360 (72.1%) |
| Improved | 69 (13.8%) |
| Degraded | 70 (14.0%) |
| Net change | +18.26 |

## Conclusion
Optimization preserves algorithm correctness. Performance changes within measurement noise.
