//! draw.io compressed 格式后处理。

/// 将完整 .drawio XML 压缩为 draw.io compressed 格式。
///
/// 算法：提取 `<diagram>...</diagram>` 内部的 `<mxGraphModel>` XML，
/// deflate 压缩后 base64 编码，替换原 `<diagram>` 内容并标记 `compressed="true"`。
#[cfg(feature = "compressed-drawio")]
pub(crate) fn compress_drawio(xml: &str) -> String {
    use base64::Engine;
    use flate2::write::DeflateEncoder;
    use flate2::Compression;
    use std::io::Write;

    // 提取 <diagram ...> 和 </diagram> 之间的内容
    let Some(diagram_start) = xml.find("<diagram") else { return xml.to_string(); };
    let Some(diagram_content_start) = xml[diagram_start..].find('>').map(|i| diagram_start + i + 1) else { return xml.to_string(); };
    let Some(diagram_end) = xml.find("</diagram>") else { return xml.to_string(); };

    let inner_xml = &xml[diagram_content_start..diagram_end];

    // Deflate 压缩
    let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
    if encoder.write_all(inner_xml.as_bytes()).is_err() {
        return xml.to_string();
    }
    let compressed = match encoder.finish() {
        Ok(data) => data,
        Err(_) => return xml.to_string(),
    };

    // Base64 编码
    let encoded = base64::engine::general_purpose::STANDARD.encode(&compressed);

    // URL encode 特殊字符（draw.io 要求）
    let url_encoded = encoded
        .replace('+', "%2B")
        .replace('/', "%2F")
        .replace('=', "%3D");

    // 重建 <diagram> 标签
    let diagram_tag_start = &xml[diagram_start..diagram_content_start - 1]; // 不含 '>'
    // 在原始 <diagram ...> 标签中添加 compressed="true"
    let new_diagram_tag = format!(r#"{} compressed="true">"#, diagram_tag_start.replace(r#" compressed="true""#, ""));

    format!(
        "{}{}{}{}",
        &xml[..diagram_start],
        new_diagram_tag,
        url_encoded,
        &xml[diagram_end..],
    )
}

/// 未启用 compressed-drawio feature 时的回退：直接返回原始 XML。
#[cfg(not(feature = "compressed-drawio"))]
pub(crate) fn compress_drawio(xml: &str) -> String {
    xml.to_string()
}
