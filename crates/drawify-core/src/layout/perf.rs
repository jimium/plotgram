//! 布局性能日志（WASM 上禁用：`std::time::Instant` 在 wasm32 不可用）。

#[cfg(not(target_arch = "wasm32"))]
pub use std::time::Instant;

#[cfg(target_arch = "wasm32")]
mod wasm_instant {
    use std::time::Duration;

    #[derive(Copy, Clone)]
    pub struct Instant;

    impl Instant {
        #[inline]
        pub fn now() -> Self {
            Instant
        }

        #[inline]
        pub fn elapsed(self) -> Duration {
            Duration::ZERO
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm_instant::Instant;

#[macro_export]
macro_rules! perf_log {
    ($($arg:tt)*) => {
        #[cfg(not(target_arch = "wasm32"))]
        eprintln!($($arg)*);
    };
}
