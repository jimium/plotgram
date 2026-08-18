//! Phase 4 / D4-1：group 几何写入可观测计数。
//!
//! 一次 layout run 中对 `layout.groups` 的**整表或批量改写 pass** 次数。
//! 退出目标：主计数 = 1（`materialize`；canvas 刚体平移不另计；PRS 另计 `prs_writes`）。
//!
//! D4-1：凡**重算/改写相对几何**的生产函数均须 `record_group_write_at`；
//! 画布整体刚体平移属 materialize 令牌生命周期，不计第二次写。

use std::cell::{Cell, RefCell};

thread_local! {
    static GROUP_WRITES: Cell<u32> = const { Cell::new(0) };
    static PRS_GROUP_WRITES: Cell<u32> = const { Cell::new(0) };
    static IN_PRS: Cell<bool> = const { Cell::new(false) };
    /// 主路径写入站点（确定性追加顺序 = 调用序）。
    static WRITE_SITES: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
    static PRS_WRITE_SITES: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

/// 新 layout run 开始时清零。
pub fn reset_group_write_counters() {
    GROUP_WRITES.with(|c| c.set(0));
    PRS_GROUP_WRITES.with(|c| c.set(0));
    IN_PRS.with(|c| c.set(false));
    WRITE_SITES.with(|s| s.borrow_mut().clear());
    PRS_WRITE_SITES.with(|s| s.borrow_mut().clear());
}

pub fn enter_prs_window() {
    IN_PRS.with(|c| c.set(true));
}

pub fn leave_prs_window() {
    IN_PRS.with(|c| c.set(false));
}

/// 记录一次 group 几何写入（无站点标签时用 `"?"`）。
pub fn record_group_write() {
    record_group_write_at("?");
}

/// 记录一次 group 几何写入，并记下调用站点（诊断用）。
pub fn record_group_write_at(site: &'static str) {
    if IN_PRS.with(|c| c.get()) {
        PRS_GROUP_WRITES.with(|c| c.set(c.get().saturating_add(1)));
        PRS_WRITE_SITES.with(|s| s.borrow_mut().push(site));
    } else {
        GROUP_WRITES.with(|c| c.set(c.get().saturating_add(1)));
        WRITE_SITES.with(|s| s.borrow_mut().push(site));
    }
}

pub fn group_write_count() -> u32 {
    GROUP_WRITES.with(|c| c.get())
}

pub fn prs_group_write_count() -> u32 {
    PRS_GROUP_WRITES.with(|c| c.get())
}

fn sites_joined(prs: bool) -> String {
    let cell = if prs { &PRS_WRITE_SITES } else { &WRITE_SITES };
    cell.with(|s| s.borrow().join(" → "))
}

/// 正式 route 前：始终打摘要；主计数 > threshold 时抬 WARN。
///
/// 工程收口棘轮：默认 threshold=1（单次 materialize）。
/// 设 `TAUTCORE_STRICT_GROUP_WRITES=1` 时超阈 panic。
pub fn warn_if_group_writes_excessive(threshold: u32) {
    let n = group_write_count();
    let prs = prs_group_write_count();
    let sites = sites_joined(false);
    let prs_sites = sites_joined(true);
    crate::perf_log!(
        "[group_write] main={} prs={} sites=[{}] prs_sites=[{}]",
        n,
        prs,
        sites,
        prs_sites
    );
    if n > threshold {
        crate::perf_log!(
            "[warn] group_writes={} > threshold={} (materialize-only ratchet)",
            n,
            threshold
        );
        if std::env::var_os("TAUTCORE_STRICT_GROUP_WRITES").is_some() {
            panic!("group_writes={n} > threshold={threshold}; sites=[{sites}]");
        }
    }
}
