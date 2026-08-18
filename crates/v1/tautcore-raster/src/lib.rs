//! Tautcore SVG 栅格化库:将 SVG 字符串转为 PNG / WebP 位图。
//!
//! 独立于 tautcore-core 的叶子 crate,输入就是 SVG 字符串,
//! 通过 usvg/resvg 栅格化为像素,再由 image crate 编码为目标格式。
//!
//! 字体目录解析优先级(高 → 低):
//! 1. [`RasterOptions::fonts_dir`](如 CLI `--fonts-dir`)
//! 2. 环境变量 [`FONTS_DIR_ENV_VAR`](`TAUTCORE_FONTS_DIR`)
//! 3. 当前工作目录下的 `fonts/`

use std::path::{Path, PathBuf};
use std::sync::Arc;

use usvg::fontdb::Database;

/// 环境变量名,用于指定字体文件目录
pub const FONTS_DIR_ENV_VAR: &str = "TAUTCORE_FONTS_DIR";

/// 栅格化错误
#[derive(Debug, thiserror::Error)]
pub enum RasterError {
    #[error("failed to parse svg: {0}")]
    SvgParse(String),
    #[error("svg dimensions are zero")]
    ZeroDimensions,
    #[error("failed to allocate pixmap")]
    PixmapAlloc,
    #[error("failed to build image buffer")]
    ImageBuffer,
    #[error("failed to encode {format}: {message}")]
    Encode { format: &'static str, message: String },
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, RasterError>;

/// 目标位图格式
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RasterFormat {
    Png,
    Webp,
}

impl RasterFormat {
    /// 从格式名 / 文件扩展名解析(不区分大小写)
    pub fn from_str(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "png" => Some(Self::Png),
            "webp" => Some(Self::Webp),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Webp => "webp",
        }
    }

    pub fn file_extension(self) -> &'static str {
        self.name()
    }

    fn image_format(self) -> image::ImageFormat {
        match self {
            Self::Png => image::ImageFormat::Png,
            Self::Webp => image::ImageFormat::WebP,
        }
    }
}

/// 栅格化选项
#[derive(Debug, Clone, Default)]
pub struct RasterOptions {
    /// 自定义字体目录(覆盖 `TAUTCORE_FONTS_DIR` 环境变量与 `cwd/fonts/`)
    pub fonts_dir: Option<PathBuf>,
}

impl RasterOptions {
    /// 解析实际生效的字体目录(显式参数 > 环境变量 > `cwd/fonts/`)
    pub fn resolved_fonts_dir(&self) -> PathBuf {
        self.fonts_dir
            .clone()
            .or_else(fonts_dir_from_env)
            .unwrap_or_else(default_fonts_dir)
    }
}

/// 默认字体目录:当前工作目录下的 `fonts/`
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

/// 构建 usvg 选项,包含系统字体与自定义字体目录中的 CJK 字体
fn build_usvg_options(opts: &RasterOptions) -> usvg::Options<'static> {
    let mut fontdb = Database::new();
    fontdb.load_system_fonts();
    load_fonts_from_dir(&mut fontdb, &opts.resolved_fonts_dir());

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

/// 将 SVG 字符串栅格化为 RGBA 像素缓冲,返回 `(width, height, image_buffer)`
fn svg_to_image(
    svg_data: &str,
    opts: &RasterOptions,
) -> Result<image::ImageBuffer<image::Rgba<u8>, Vec<u8>>> {
    let usvg_opts = build_usvg_options(opts);
    let tree = usvg::Tree::from_str(svg_data, &usvg_opts)
        .map_err(|e| RasterError::SvgParse(e.to_string()))?;

    let width = tree.size().width() as u32;
    let height = tree.size().height() as u32;
    if width == 0 || height == 0 {
        return Err(RasterError::ZeroDimensions);
    }

    let mut pixmap =
        resvg::tiny_skia::Pixmap::new(width, height).ok_or(RasterError::PixmapAlloc)?;

    let mut pixmap_ref = pixmap.as_mut();
    resvg::render(&tree, resvg::tiny_skia::Transform::default(), &mut pixmap_ref);

    image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(width, height, pixmap.data().to_vec())
        .ok_or(RasterError::ImageBuffer)
}

/// 将 SVG 字符串栅格化并编码为目标格式的字节
pub fn rasterize_svg(
    svg_data: &str,
    format: RasterFormat,
    opts: &RasterOptions,
) -> Result<Vec<u8>> {
    let img = svg_to_image(svg_data, opts)?;

    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    img.write_to(&mut cursor, format.image_format())
        .map_err(|e| RasterError::Encode {
            format: format.name(),
            message: e.to_string(),
        })?;

    Ok(buf)
}

/// 将 SVG 字符串栅格化并保存到文件
pub fn rasterize_svg_to_file(
    svg_data: &str,
    format: RasterFormat,
    opts: &RasterOptions,
    output_path: &Path,
) -> Result<()> {
    let bytes = rasterize_svg(svg_data, format, opts)?;
    std::fs::write(output_path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIMPLE_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="48">
  <rect x="4" y="4" width="56" height="40" fill="#4a90d9" stroke="#333"/>
</svg>"##;

    #[test]
    fn rasterize_svg_emits_expected_magic_bytes() {
        let cases: &[(RasterFormat, &[u8])] = &[
            (RasterFormat::Png, &[0x89, b'P', b'N', b'G']),
            (RasterFormat::Webp, b"RIFF"),
        ];
        for (format, magic) in cases {
            let bytes = rasterize_svg(SIMPLE_SVG, *format, &RasterOptions::default())
                .unwrap_or_else(|e| panic!("{} rasterize failed: {e}", format.name()));
            assert!(
                bytes.starts_with(magic),
                "{} output missing magic bytes",
                format.name()
            );
            if *format == RasterFormat::Webp {
                assert_eq!(&bytes[8..12], b"WEBP");
            }
            assert!(bytes.len() > 32);
        }
    }

    #[test]
    fn rasterize_svg_rejects_invalid_input() {
        let cases: &[(&str, &str)] = &[
            ("not svg at all", "parse error"),
            (
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="0" height="0"></svg>"#,
                "zero dimensions",
            ),
        ];
        for (svg, desc) in cases {
            let result = rasterize_svg(svg, RasterFormat::Png, &RasterOptions::default());
            assert!(result.is_err(), "expected error for {desc}");
        }
    }
}
