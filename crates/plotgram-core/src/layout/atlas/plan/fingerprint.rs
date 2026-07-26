//! 稳定指纹（23 号文 Stage 1 交付 1.2）：Plan 的规范字节编码 + FNV-1a 64。
//!
//! **稳定性保证**：不用 `std::collections::hash_map::DefaultHasher`（跨 Rust
//! 版本无稳定性承诺），FNV 常量硬编码——同一 Plan 的指纹跨双跑、跨构建、
//! 跨平台一致（AGENTS.md §2 确定性红线的可验证形式）。
//!
//! **编码唯一性**：按固定字段顺序写 section 标签 + 长度前缀 + `BTreeMap`
//! 迭代序条目（键有序 → 同内容必同编码，与插入顺序无关）。
//!
//! **决策口径**（与 [`super::diff`] / [`Plan::semantic_eq`] 一致）：`provenance`
//! 不参与（溯源是元数据非决策）；`bundles` 按规范序编码（Vec 顺序不携带
//! 语义，同 diff 的集合差口径）；`PortRef::slot_id` 不参与。
//!
//! 非密码学哈希：仅供一致性判定与未来增量缓存键，不抗恶意碰撞。

use super::{Plan, PortRef, canonical_bundles};
use crate::layout::atlas::channel::PortSide;

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// FNV-1a 64 流式写入器。
struct Fnv1a(u64);

impl Fnv1a {
    fn new() -> Self {
        Self(FNV_OFFSET)
    }

    fn bytes(&mut self, data: &[u8]) {
        for &b in data {
            self.0 ^= u64::from(b);
            self.0 = self.0.wrapping_mul(FNV_PRIME);
        }
    }

    /// section 标签：区隔字段，防止相邻字段的编码串位。
    fn tag(&mut self, t: u8) {
        self.bytes(&[t]);
    }

    fn u64(&mut self, v: u64) {
        self.bytes(&v.to_le_bytes());
    }

    fn usize(&mut self, v: usize) {
        self.u64(v as u64);
    }

    /// 长度前缀字符串（防止 "ab"+"c" 与 "a"+"bc" 同码）。
    fn str(&mut self, s: &str) {
        self.usize(s.len());
        self.bytes(s.as_bytes());
    }
}

/// 端口侧的稳定判别值（显式 match，不依赖编译器判别式布局）。
fn side_code(side: PortSide) -> u8 {
    match side {
        PortSide::MainLow => 0,
        PortSide::MainHigh => 1,
        PortSide::CrossLow => 2,
        PortSide::CrossHigh => 3,
    }
}

/// 端口引用编码：`(node, side, slot_index)` 语义身份；`slot_id` **不参与**
/// （基底重建后 PortSlotId 可能重编号，指纹只认语义身份，见 `PortRef` doc）。
fn write_port(h: &mut Fnv1a, p: &PortRef) {
    h.str(&p.node);
    h.bytes(&[side_code(p.side)]);
    h.u64(u64::from(p.slot_index));
}

impl Plan {
    /// 规范编码指纹（FNV-1a 64）。同内容 Plan（无关插入顺序）恒同值；
    /// 决策口径见模块头（provenance / slot_id / bundles 顺序不参与）。
    pub fn fingerprint(&self) -> u64 {
        let mut h = Fnv1a::new();

        h.tag(0x01); // substrate
        h.usize(self.substrate.rank_count);
        h.usize(self.substrate.order_count);

        h.tag(0x02); // node_slots
        h.usize(self.node_slots.len());
        for (node, slot) in &self.node_slots {
            h.str(node);
            h.usize(slot.rank);
            h.usize(slot.order);
        }

        h.tag(0x03); // group_scopes
        h.usize(self.group_scopes.len());
        for (group, scope) in &self.group_scopes {
            h.str(group);
            match &scope.parent {
                Some(p) => {
                    h.bytes(&[1]);
                    h.str(p);
                }
                None => h.bytes(&[0]),
            }
            h.usize(scope.ranks.0);
            h.usize(scope.ranks.1);
            h.usize(scope.orders.0);
            h.usize(scope.orders.1);
        }

        h.tag(0x04); // ports
        h.usize(self.ports.len());
        for (&edge, ep) in &self.ports {
            h.usize(edge);
            write_port(&mut h, &ep.from);
            write_port(&mut h, &ep.to);
        }

        h.tag(0x05); // gates
        h.usize(self.gates.len());
        for (&edge, gates) in &self.gates {
            h.usize(edge);
            h.usize(gates.len());
            for g in gates {
                h.u64(u64::from(g.0));
            }
        }

        h.tag(0x06); // channels
        h.usize(self.channels.len());
        for (&edge, tracks) in &self.channels {
            h.usize(edge);
            h.usize(tracks.len());
            for t in tracks {
                h.u64(u64::from(t.0));
            }
        }

        h.tag(0x07); // bundles（规范序：Vec 顺序不参与，见 canonical_bundles）
        let bundles = canonical_bundles(&self.bundles);
        h.usize(bundles.len());
        for b in bundles {
            h.usize(b.edges.len());
            for &e in &b.edges {
                h.usize(e);
            }
            h.usize(b.suffix.len());
            for t in &b.suffix {
                h.u64(u64::from(t.0));
            }
        }

        h.0
    }
}
