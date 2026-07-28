//! plotgram-raster CLI:将 SVG 文件转为 PNG / WebP。
//!
//! 用法:
//! ```text
//! plotgram-raster input.svg -o out.png            # 格式按 -o 扩展名推断
//! plotgram-raster input.svg -o out.bin -f webp    # --format 覆盖推断
//! cat input.svg | plotgram-raster - -o out.png    # 从 stdin 读入
//! ```

use std::io::Read;
use std::path::{Path, PathBuf};

use clap::Parser;
use plotgram_raster::{rasterize_svg_to_file, RasterFormat, RasterOptions};

#[derive(Parser)]
#[command(name = "plotgram-raster", about = "将 SVG 转为 PNG / WebP 位图", version)]
struct Cli {
    /// 输入 SVG 文件路径,使用 `-` 表示从 stdin 读取
    input: String,

    /// 输出文件路径
    #[arg(short, long)]
    output: PathBuf,

    /// 输出格式 (png/webp);缺省时按输出文件扩展名推断
    #[arg(short, long)]
    format: Option<String>,

    /// 字体文件目录(覆盖 PLOTGRAM_FONTS_DIR 环境变量;均未设置时使用 cwd/fonts/)
    #[arg(long = "fonts-dir")]
    fonts_dir: Option<PathBuf>,
}

fn resolve_format(cli_format: Option<&str>, output: &Path) -> RasterFormat {
    if let Some(value) = cli_format {
        return RasterFormat::from_str(value).unwrap_or_else(|| {
            eprintln!("错误: 不支持的格式 '{}'(可选: png, webp)", value);
            std::process::exit(1);
        });
    }
    let ext = output
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or("");
    RasterFormat::from_str(ext).unwrap_or_else(|| {
        eprintln!(
            "错误: 无法从输出文件 '{}' 推断格式,请使用 --format 指定(可选: png, webp)",
            output.display()
        );
        std::process::exit(1);
    })
}

fn read_input(input: &str) -> std::io::Result<String> {
    if input == "-" {
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        Ok(buf)
    } else {
        std::fs::read_to_string(input)
    }
}

fn main() {
    let cli = Cli::parse();

    let format = resolve_format(cli.format.as_deref(), &cli.output);

    let svg_data = read_input(&cli.input).unwrap_or_else(|e| {
        eprintln!("错误: 无法读取输入 '{}': {}", cli.input, e);
        std::process::exit(1);
    });

    let opts = RasterOptions {
        fonts_dir: cli.fonts_dir,
    };
    let fonts_dir = opts.resolved_fonts_dir();
    if !fonts_dir.is_dir() {
        eprintln!(
            "警告: 字体目录 {} 不存在,仅使用系统字体(CJK 文本可能显示异常)",
            fonts_dir.display()
        );
    }

    rasterize_svg_to_file(&svg_data, format, &opts, &cli.output).unwrap_or_else(|e| {
        eprintln!("错误: 栅格化失败: {}", e);
        std::process::exit(1);
    });

    println!(
        "{} 已写入: {}",
        format.name().to_uppercase(),
        cli.output.display()
    );
}
