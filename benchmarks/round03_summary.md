# Round 03 & 04: Large Graph Degradation Optimization — ABANDONED

## Round 03: node <=20 AND edges >25 threshold

### Changes
Added threshold in `assign_component_ranks_network_simplex`:
```rust
const NS_SMALL_NODE_THRESHOLD: usize = 20;
const NS_DENSE_EDGE_THRESHOLD: usize = 25;

let should_skip_simplex = component.len() <= NS_SMALL_NODE_THRESHOLD
    && component_edge_count > NS_DENSE_EDGE_THRESHOLD;
```

### Results
- ❌ `c.k8s-tenant-isolation`: score 65.67 → 62.23 (-3.44), time 1204 → 3230 μs (SLOWER!)
- Mixed performance changes

### Conclusion
Threshold too lenient, caused regression.

---

## Round 04: node <=15 threshold only

### Changes
Simplified threshold: skip simplex if component.len() <= 15

### Results
- ❌ `n.school-schema`: score 88.14 → 75.22 (-12.92!)
- ❌ `c.ecommerce-schema`: score 81.39 → 71.60 (-9.79!)
- ❌ Multiple diagrams degraded
- ❌ Overall speedup: -6.49% (slower, not faster!)

### Root Cause
The `assign_component_ranks_network_simplex` is called per weak component, not per diagram.
When a diagram splits into multiple small components, each gets different treatment.
Skipping simplex for ANY component changes the ranking, which cascades into
different layer orderings and coordinate assignments, ultimately degrading scores.

---

## Conclusion

The large graph degradation approach is fundamentally flawed for this codebase:

1. **Ranking quality matters for ALL sizes**: Even 5-node diagrams benefit from simplex
2. **Component splitting is unpredictable**: A 10-node diagram might split into components of 3+7
3. **Downstream effects**: Different rankings cascade into different orderings and coordinates
4. **Time savings are negligible**: Small graphs are already fast enough

**Only Round 02 optimizations are kept.**
