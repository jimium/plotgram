# Performance Benchmark Data

This folder stores benchmark results from each optimization round.

## File Naming Convention
```
round{XX}_{description}.json
```

## Rounds

### Round 01: Baseline
- **File**: `round01_baseline.json`
- **Date**: Before any performance optimizations
- **Description**: Original algorithm performance without optimizations

### Round 02: sugiyama_v2 + visibility Dijkstra Optimization ✅ KEEP
- **File**: `round02_sugiyama_visibility_opt.json`
- **Date**: 2026-06-13
- **Changes**:
  - `sugiyama_v2/rank.rs`: Pre-computed adjacency tables, Vec<bool> instead of HashSet
  - `visibility.rs`: BinaryHeap for Dijkstra instead of linear scan
- **Results**:
  - sugiyama-v2: **100% score consistency** (51/51 diagrams)
  - Performance: within measurement noise
- **Status**: ✅ **KEPT** - Algorithm correctness preserved

### Round 03: Large Graph Degradation (node <=20, edges >25 threshold) ❌ ABANDONED
- **File**: `round03_large_graph_degradation.json`
- **Date**: 2026-06-13
- **Changes**: Added threshold to skip network simplex for small/dense graphs
- **Results**:
  - Score regression on `c.k8s-tenant-isolation`: -3.44
  - Mixed performance results
- **Status**: ❌ **ABANDONED** - Hurt quality

### Round 04: Large Graph Degradation (node <=15 threshold) ❌ ABANDONED
- **File**: `round04_final_verify.json` (same as round02, used for verification)
- **Changes**: More aggressive threshold (node <=15)
- **Results**:
  - `n.school-schema`: score dropped from 88.14 to 75.22 (-12.92)
  - `c.ecommerce-schema`: score dropped from 81.39 to 71.60 (-9.79)
  - Multiple diagrams degraded
- **Status**: ❌ **ABANDONED** - Severely hurt quality

## Conclusion

The large graph degradation optimization (Round 03/04) was abandoned because:

1. **Quality degradation**: Small/medium graphs benefit significantly from network simplex
2. **Threshold tuning is difficult**: Even with 15-node threshold, quality degrades
3. **Time savings are marginal**: The original network simplex is already fast enough for small graphs

**Only Round 02 optimizations are kept:**
- Pre-computed adjacency + Vec<bool> in sugiyama_v2
- BinaryHeap Dijkstra in visibility

## Comparison Script

```bash
python3 << 'EOF'
import json

with open('benchmarks/round01_baseline.json') as f:
    baseline = json.load(f)
with open('benchmarks/round02_sugiyama_visibility_opt.json') as f:
    optimized = json.load(f)

# Compare scores and performance
...
EOF
```
