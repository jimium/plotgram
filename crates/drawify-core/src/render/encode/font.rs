//! 字体加载模块
//!
//! 提供 CJK 字体支持，解决 resvg 渲染 PNG/WebP 时中文显示问题。
//!
//! 字体目录解析优先级（高 → 低）：
//! 1. [`set_fonts_dir`]（如 CLI `--fonts-dir`）
//! 2. 环境变量 [`FONTS_DIR_ENV_VAR`]（`DRAWIFY_FONTS_DIR`）
//! 3. 当前工作目录下的 `fonts/`

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use usvg::fontdb::Database;

/// 环境变量名，用于指定字体文件目录
pub const FONTS_DIR_ENV_VAR: &str = "DRAWIFY_FONTS_DIR";

static FONTS_DIR: OnceLock<PathBuf> = OnceLock::new();

/// 默认字体目录：当前工作目录下的 `fonts/`
pub fn default_fonts_dir() -> PathBuf {
    std::env::current_dir()
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("fonts")
}

/// 从环境变量读取字体目录
pub fn fonts_dir_from_env() -> Option<PathBuf> {
    std::env::var(FONTS_DIR_ENV_VAR)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

/// 设置字体文件目录（需在首次调用 [`build_usvg_options`] 之前设置）
pub fn set_fonts_dir(path: impl Into<PathBuf>) {
    let _ = FONTS_DIR.set(path.into());
}

/// 获取当前使用的字体目录
pub fn fonts_dir() -> PathBuf {
    FONTS_DIR
        .get()
        .cloned()
        .or_else(fonts_dir_from_env)
        .unwrap_or_else(default_fonts_dir)
}

/// 构建 usvg 选项，包含系统字体与自定义字体目录中的 CJK 字体
pub fn build_usvg_options() -> usvg::Options<'static> {
    let mut fontdb = Database::new();
    fontdb.load_system_fonts();
    load_fonts_from_dir(&mut fontdb, &fonts_dir());

    usvg::Options {
        fontdb: Arc::new(fontdb),
        ..Default::default()
    }
}

fn load_fonts_from_dir(fontdb: &mut Database, dir: &Path) {
    if !dir.is_dir() {
        return;
    }
    fontdb.load_fonts_dir(dir);
}
