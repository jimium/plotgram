//! 栅格化共享逻辑
//!
//! 将 SVG 字符串通过 usvg/resvg 栅格化为位图像素，
//! 再通过 image crate 编码为目标格式。
//! png 与 webp 共享此逻辑，仅格式参数不同。

use crate::error::{DrawifyError, Result};
use crate::render::encode::font::build_usvg_options;

/// 将 SVG 字符串栅格化为位图像素
///
/// 返回 `(width, height, image_buffer)`，
/// 其中 `image_buffer` 为 `image::ImageBuffer<image::Rgba<u8>, Vec<u8>>`。
pub fn svg_to_pixmap(
    svg_data: &str,
    format_name: &str,
) -> Result<(u32, u32, image::ImageBuffer<image::Rgba<u8>, Vec<u8>>)> {
    let opts = build_usvg_options();
    let tree = usvg::Tree::from_str(svg_data, &opts)
        .map_err(|e| DrawifyError::render_internal_msg(format!("failed to parse svg for {format_name} encoding: {e}")))?;

    let width = tree.size().width() as u32;
    let height = tree.size().height() as u32;
    if width == 0 || height == 0 {
        return Err(DrawifyError::render_internal_msg(format!(
            "{format_name} svg dimensions are zero"
        )));
    }

    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| DrawifyError::render_internal_msg(format!("failed to allocate {format_name} pixmap")))?;

    let mut pixmap_ref = pixmap.as_mut();
    resvg::render(&tree, resvg::tiny_skia::Transform::default(), &mut pixmap_ref);

    let img = image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(
        width,
        height,
        pixmap.data().to_vec(),
    )
    .ok_or_else(|| {
        DrawifyError::render_internal_msg(format!("failed to build {format_name} image buffer"))
    })?;

    Ok((width, height, img))
}

/// 将 SVG 字符串栅格化并编码为目标格式的字节
pub fn render_svg_to_image_bytes(
    svg_data: &str,
    format: image::ImageFormat,
    format_name: &str,
) -> Result<Vec<u8>> {
    let (_width, _height, img) = svg_to_pixmap(svg_data, format_name)?;

    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    img.write_to(&mut cursor, format)
        .map_err(|e| DrawifyError::render_internal_msg(format!("failed to encode {format_name}: {e}")))?;

    Ok(buf)
}

/// 将 SVG 字符串栅格化并保存到文件
pub fn render_svg_to_image_file(svg_data: &str, output_path: &str) -> Result<()> {
    let opts = build_usvg_options();
    let tree = usvg::Tree::from_str(svg_data, &opts)
        .map_err(|e| DrawifyError::render_internal_msg(format!("failed to parse svg for rasterization: {e}")))?;

    let width = tree.size().width() as u32;
    let height = tree.size().height() as u32;
    if width == 0 || height == 0 {
        return Err(DrawifyError::render_internal_msg("svg dimensions are zero"));
    }

    let mut pixmap = resvg::tiny_skia::Pixmap::new(width, height)
        .ok_or_else(|| DrawifyError::render_internal_msg("failed to allocate pixmap"))?;

    let mut pixmap_ref = pixmap.as_mut();
    resvg::render(&tree, resvg::tiny_skia::Transform::default(), &mut pixmap_ref);

    let img = image::ImageBuffer::<image::Rgba<u8>, Vec<u8>>::from_raw(
        width,
        height,
        pixmap.data().to_vec(),
    )
    .ok_or_else(|| DrawifyError::render_internal_msg("failed to build image buffer"))?;

    img.save(output_path)
        .map_err(|e| DrawifyError::render_internal_msg(format!("failed to save file: {e}")))?;

    Ok(())
}
