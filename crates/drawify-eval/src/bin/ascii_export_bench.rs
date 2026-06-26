use drawify_core::ast::{PreparedDiagram, RawDiagram};
use drawify_core::prepare::StyleRequest;
use drawify_core::render::encode::ascii::{
    export_reader_to_writer, export_string, AsciiExportMetadata, AsciiExportOptions,
    AsciiNonAsciiPolicy,
};
use drawify_core::render::{RenderFormat, RenderRequest};
use serde::Serialize;
use std::fs;
use std::io::{Cursor, sink};
use std::time::Instant;

#[derive(Debug, Serialize)]
struct BenchReport {
    benchmark: &'static str,
    generated_at_unix_ms: u128,
    environment: BenchEnvironment,
    scenarios: Vec<BenchScenario>,
}

#[derive(Debug, Serialize)]
struct BenchEnvironment {
    rust_package: &'static str,
    target_os: &'static str,
    target_arch: &'static str,
}

#[derive(Debug, Serialize)]
struct BenchScenario {
    name: &'static str,
    mode: &'static str,
    iterations: usize,
    input_bytes: usize,
    output_bytes: usize,
    average_ms: f64,
    min_ms: f64,
    max_ms: f64,
    throughput_mib_per_s: f64,
    bounded_forward_buffer_bytes: usize,
    sample_metadata: AsciiExportMetadata,
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let output_path = args
        .iter()
        .position(|arg| arg == "--output" || arg == "-o")
        .and_then(|index| args.get(index + 1).cloned())
        .unwrap_or_else(|| "benchmarks/ascii_export_round01.json".to_string());

    let report = BenchReport {
        benchmark: "ascii_export_round01",
        generated_at_unix_ms: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_millis())
            .unwrap_or_default(),
        environment: BenchEnvironment {
            rust_package: env!("CARGO_PKG_NAME"),
            target_os: std::env::consts::OS,
            target_arch: std::env::consts::ARCH,
        },
        scenarios: vec![
            bench_string_normalization(),
            bench_stream_forwarding(),
            bench_diagram_rendering(),
        ],
    };

    let json = serde_json::to_string_pretty(&report).expect("serialize benchmark report");
    fs::write(&output_path, json).expect("write benchmark report");
    println!("ASCII benchmark report written to {output_path}");
}

fn bench_string_normalization() -> BenchScenario {
    let input = "Hello\t世界 → Drawify\r\n".repeat(8_192);
    let options = AsciiExportOptions {
        non_ascii_policy: AsciiNonAsciiPolicy::Approximate,
        include_metadata: false,
        chunk_size: 2048,
        ..AsciiExportOptions::default()
    };
    let iterations = 50;
    let mut samples = Vec::with_capacity(iterations);
    let mut output_bytes = 0;
    let mut sample_metadata = None;

    for _ in 0..iterations {
        let start = Instant::now();
        let result = export_string(&input, &options).expect("normalize string input");
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        output_bytes = result.text.len();
        sample_metadata = Some(result.metadata);
        samples.push(elapsed);
    }

    scenario_from_samples(
        "normalize_mixed_utf8_string",
        "string",
        iterations,
        input.len(),
        output_bytes,
        options.normalized_chunk_size(),
        sample_metadata.expect("string benchmark metadata"),
        &samples,
    )
}

fn bench_stream_forwarding() -> BenchScenario {
    let input = "stream中line\r\n".repeat(700_000).into_bytes();
    let options = AsciiExportOptions {
        chunk_size: 4 * 1024,
        include_metadata: false,
        ..AsciiExportOptions::default()
    };
    let iterations = 8;
    let mut samples = Vec::with_capacity(iterations);
    let mut sample_metadata = None;

    for _ in 0..iterations {
        let mut reader = Cursor::new(&input);
        let mut writer = sink();
        let start = Instant::now();
        let metadata = export_reader_to_writer(&mut reader, &mut writer, &options)
            .expect("stream large utf8 input");
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        sample_metadata = Some(metadata);
        samples.push(elapsed);
    }

    let metadata = sample_metadata.expect("stream benchmark metadata");
    scenario_from_samples(
        "stream_large_utf8_to_sink",
        "stream",
        iterations,
        input.len(),
        metadata.output_bytes,
        options.normalized_chunk_size(),
        metadata,
        &samples,
    )
}

fn bench_diagram_rendering() -> BenchScenario {
    let source = r#"
diagram sequence {
    title: "ASCII Playground"
    config {
        direction: left-to-right
    }

    entity client "客户端" { type: boundary }
    entity worker "Worker 中心" { type: control }

    client -> worker "GET /ascii"
    worker --> client "done"
}
"#;

    let prepared = prepare_diagram(source);
    let iterations = 40;
    let mut samples = Vec::with_capacity(iterations);
    let mut output_bytes = 0;
    let mut sample_metadata = None;

    for _ in 0..iterations {
        let mut request = RenderRequest::new(&prepared, RenderFormat::Ascii);
        request.ascii_options = AsciiExportOptions {
            include_metadata: true,
            ..AsciiExportOptions::default()
        };

        let start = Instant::now();
        let result = drawify_core::render::encode::ascii::encode_with_report(&request)
            .expect("render diagram into ascii");
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        output_bytes = result.text.len();
        sample_metadata = Some(result.metadata);
        samples.push(elapsed);
    }

    scenario_from_samples(
        "render_sequence_ascii",
        "render",
        iterations,
        source.len(),
        output_bytes,
        0,
        sample_metadata.expect("render benchmark metadata"),
        &samples,
    )
}

fn prepare_diagram(source: &str) -> PreparedDiagram {
    let raw = drawify_core::pipeline::parse(source).expect("parse diagram source");
    drawify_core::pipeline::prepare(RawDiagram(raw.into_inner()), &StyleRequest::default())
        .expect("prepare diagram")
        .diagram
}

fn scenario_from_samples(
    name: &'static str,
    mode: &'static str,
    iterations: usize,
    input_bytes: usize,
    output_bytes: usize,
    bounded_forward_buffer_bytes: usize,
    sample_metadata: AsciiExportMetadata,
    samples: &[f64],
) -> BenchScenario {
    let total_ms: f64 = samples.iter().sum();
    let average_ms = total_ms / samples.len() as f64;
    let min_ms = samples.iter().copied().fold(f64::INFINITY, f64::min);
    let max_ms = samples.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mib = input_bytes as f64 / (1024.0 * 1024.0);
    let throughput_mib_per_s = if average_ms <= f64::EPSILON {
        0.0
    } else {
        mib / (average_ms / 1000.0)
    };

    BenchScenario {
        name,
        mode,
        iterations,
        input_bytes,
        output_bytes,
        average_ms,
        min_ms,
        max_ms,
        throughput_mib_per_s,
        bounded_forward_buffer_bytes,
        sample_metadata,
    }
}
