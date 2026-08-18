//! 几何量溯源（Stage 0 交付 0.1，23 号文 §2）。
//!
//! 记录「这个几何量是谁写的」——手册「先追写权」法则的类型化：任何进入
//! 度量相 / 落笔相的几何量都应能回答产地。Stage 7 的新门禁要求
//! Provenance 覆盖率 100%（23 号文 §9）。
//!
//! **与 [`super::plan::Provenance`] 的分工**：plan 侧是**边级离散决策**的
//! 溯源（该边的 channels/gates 来自 ChannelRoute 还是 LegacyAdapter），
//! 本模块是**几何量**（坐标 / 尺寸 / Demand）的溯源，二者维度不同、
//! 不可互换。

use std::fmt;

/// 几何量的产地标记。
///
/// `producer` 用 `&'static str` 而非枚举：Stage 推进期写点会频繁增删，
/// 枚举会把 atlas 子树与旧管线的模块名耦合进类型系统。命名约定
/// `"模块路径:动作"`，如 `"metric/solve_axis:x"`、`"legacy/materialize"`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    /// 写入者（静态标识，见命名约定）。
    pub producer: &'static str,
    /// 可选细节（如具体节点 / 约束 id），仅用于诊断输出。
    pub detail: Option<String>,
}

impl Provenance {
    pub fn new(producer: &'static str) -> Self {
        Self {
            producer,
            detail: None,
        }
    }

    pub fn with_detail(producer: &'static str, detail: impl Into<String>) -> Self {
        Self {
            producer,
            detail: Some(detail.into()),
        }
    }
}

impl fmt::Display for Provenance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.detail {
            Some(d) => write!(f, "{} ({})", self.producer, d),
            None => f.write_str(self.producer),
        }
    }
}

/// 携带溯源的几何量。
///
/// `Deref` 到内层值：读取处无感，写入处必须显式给出产地。
#[derive(Debug, Clone, PartialEq)]
pub struct Sourced<T> {
    pub value: T,
    pub provenance: Provenance,
}

impl<T> Sourced<T> {
    pub fn new(value: T, provenance: Provenance) -> Self {
        Self { value, provenance }
    }

    /// 变换内层值，产地改记为新写入者（几何量被改写 = 产地变更）。
    pub fn map<U>(self, producer: &'static str, f: impl FnOnce(T) -> U) -> Sourced<U> {
        Sourced {
            value: f(self.value),
            provenance: Provenance::new(producer),
        }
    }
}

impl<T> std::ops::Deref for Sourced<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sourced_derefs_to_value_and_map_changes_producer() {
        let x = Sourced::new(10.0_f64, Provenance::new("test/init"));
        assert_eq!(*x, 10.0);
        assert_eq!(x.provenance.producer, "test/init");

        let y = x.map("test/scale", |v| v * 2.0);
        assert_eq!(*y, 20.0);
        assert_eq!(y.provenance.producer, "test/scale");
        assert!(y.provenance.detail.is_none());
    }

    #[test]
    fn provenance_display_includes_detail() {
        assert_eq!(Provenance::new("m:a").to_string(), "m:a");
        assert_eq!(
            Provenance::with_detail("m:a", "node n1").to_string(),
            "m:a (node n1)"
        );
    }
}
