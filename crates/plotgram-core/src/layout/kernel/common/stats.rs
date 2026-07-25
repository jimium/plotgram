//! 共享统计工具函数。

/// 计算切片的中位数（排序后取中间值）。
///
/// 对奇数长度返回正中元素，对偶数长度返回两个中间元素的算术平均。
/// 空切片触发 `debug_assert!`。
pub fn median_f64(xs: &[f64]) -> f64 {
    let n = xs.len();
    debug_assert!(n > 0);
    if n % 2 == 1 {
        xs[n / 2]
    } else {
        (xs[n / 2 - 1] + xs[n / 2]) * 0.5
    }
}
