//! Border Shell 与走廊路由的共享常量。

/// 分组边框外侧/内侧壳层厚度。
pub const GROUP_BORDER_SHELL_PAD: f64 = 12.0;

/// 端口出线 stub 豁免长度（与 orthogonal `PORT_CLEARANCE` 对齐）。
pub const PORT_STUB_CLEARANCE: f64 = 16.0;

pub(crate) const EPS: f64 = 0.1;
pub(crate) const COLLINEAR_EPS: f64 = 0.1;

const _: () = assert!(PORT_STUB_CLEARANCE == 16.0);
