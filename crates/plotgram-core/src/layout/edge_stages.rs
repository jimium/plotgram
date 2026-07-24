//! 几何冻结屏障：确保管线后处理阶段不越权修改已冻结的几何。
//!
//! - [`NodeFreeze`]：节点冻结屏障，step 10 之后不得再挪节点。
//! - [`PolylineFreeze`]：折线冻结屏障，repair 之后仅允许改 label/annotation。

/// **节点冻结屏障**：快照节点指纹，之后断言未变。
pub struct NodeFreeze {
    #[cfg(debug_assertions)]
    fingerprint: String,
}

impl NodeFreeze {
    /// 在节点冻结点采集指纹（复用 metrics 的 `node_fingerprint` 约定）。
    /// release 构建下不计算指纹，与 `debug_assert` 一同被编译掉。
    pub fn capture(result: &crate::layout::LayoutResult) -> Self {
        #[cfg(not(debug_assertions))]
        let _ = result;
        Self {
            #[cfg(debug_assertions)]
            fingerprint: crate::layout::quality::metrics::node_fingerprint(result),
        }
    }

    /// 断言节点未变；debug 构建下若被挪动即 panic（定位破坏冻结契约的上游阶段）。
    pub fn assert_unchanged(&self, result: &crate::layout::LayoutResult) {
        #[cfg(not(debug_assertions))]
        let _ = result;
        #[cfg(debug_assertions)]
        debug_assert_eq!(
            self.fingerprint,
            crate::layout::quality::metrics::node_fingerprint(result),
            "A3 节点冻结屏障被破坏：step 10 之后仍有阶段挪动了节点（应只改边几何/label）"
        );
    }
}

/// **折线冻结屏障**：快照边折线指纹，之后软校验。
pub struct PolylineFreeze {
    fingerprint: u64,
}

impl PolylineFreeze {
    /// 在折线冻结点采集指纹。
    pub fn capture(result: &crate::layout::LayoutResult) -> Self {
        Self {
            fingerprint: polyline_fingerprint(result),
        }
    }

    /// 若折点在冻结后仍变动，打 warning（不 panic）。
    pub fn warn_if_changed(&self, result: &crate::layout::LayoutResult) {
        if self.fingerprint != polyline_fingerprint(result) {
            crate::perf_log!(
                "[warn] A3 折线冻结屏障：step 16 之后 label 阶段改动了边折点（应只改 label/annotation），疑似上游 bug"
            );
        }
    }
}

/// 边折线指纹：按边序（稳定）遍历 anchor 折点，量化后 fnv1a64 哈希。
fn polyline_fingerprint(result: &crate::layout::LayoutResult) -> u64 {
    edges_fingerprint(&result.edges)
}

/// 边集几何指纹：按边序遍历 anchor 折点，量化（×100 取整）后 fnv1a64 哈希。
fn edges_fingerprint(edges: &[crate::layout::types::EdgeLayout]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    let mut mix = |v: i64| {
        for b in v.to_le_bytes() {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    };
    for edge in edges {
        for p in edge.geometry.anchor_points().iter() {
            mix((p.x * 100.0).round() as i64);
            mix((p.y * 100.0).round() as i64);
        }
        // 边界分隔符：避免相邻边折点拼接歧义。
        mix(i64::MIN);
    }
    hash
}


