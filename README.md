# Drawify

**Turn anything into a diagram — a diagram description language and rendering engine built for AI agents.**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)

Drawify is **not** a drop-in replacement for Mermaid. It is a diagram language designed from the ground up for **machine generation**: LLMs write the source, the engine handles layout, and humans read the result.

---

## Why Drawify?

Traditional diagram tools (Mermaid, PlantUML, Graphviz) were built for humans typing by hand. AI agents need something different:

| Challenge | Legacy tools | Drawify |
|-----------|--------------|---------|
| Syntax variants | Many arrow styles, implicit rules | Fixed grammar — 3 arrow types, explicit structure |
| Layout | Agent must express coordinates or hints | Semantic-first — engine infers layout automatically |
| Errors | Silent failures or opaque messages | Structured diagnostics with location and fix suggestions |
| Programmability | Text is the only artifact | AST is first-class — JSON export, semantic diff & patch |

---

## Features

- **Six diagram types** — flowchart, sequence, architecture, state, ER, and mindmap
- **Semantic entities** — declare *what* something is (`type: database`, `type: service`); the renderer picks shapes and icons
- **Automatic layout** — Sugiyama, force-directed, circular, mindmap, and sequence layouts built in
- **Multiple export formats** — SVG, PNG, WebP, ASCII, and JSON (AST)
- **Structured tooling** — validate, diff, and patch at the AST level
- **Cross-platform** — Rust core shared by CLI, HTTP server, and WASM (browser)

---

## Quick Example

```drawify
diagram flowchart {
    layout: left-to-right
    title: "Linear Flow"

    entity start "Start" { type: start }
    entity process "Process" { type: process }
    entity end "End" { type: end }

    start -> process
    process -> end
}
```

Render it:

```bash
cargo run -p drawify-cli -- render showcase/flowchart/s.linear-chain.dfy -f svg -o output.svg
```

---

## Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) 1.75 or later
- (Optional) Node.js 16+ for the playground editor

### Build

```bash
git clone https://github.com/your-org/drawify.git
cd drawify
cargo build --release
```

### Install CLI

```bash
cargo install --path crates/drawify-cli
drawify --help
```

---

## Usage

### CLI

| Command | Description |
|---------|-------------|
| `drawify render <file>` | Parse and render a `.dfy` file (`-f svg\|ascii\|png\|webp\|json`) |
| `drawify validate <file>` | Check syntax and semantics |
| `drawify export <file>` | Export the AST as JSON |
| `drawify diff -o old.dfy -n new.dfy` | Semantic diff between two files |
| `drawify patch <file> <patch.json>` | Apply an AST-level patch |

```bash
# Render to stdout (default format: SVG)
drawify render examples/my-diagram.dfy

# Validate and print diagnostics
drawify validate examples/my-diagram.dfy
```

### HTTP Server

```bash
cargo run -p drawify-server
# Listens on 0.0.0.0:6080 (override with DRAWIFY_SERVER_ADDR)
```

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check |
| `/validate` | POST | Validate source (`{ "source": "..." }`) |
| `/render` | POST | Render source (`{ "source": "...", "format": "svg" }`) |

### Playground

Browser-based live editor powered by WASM:

```bash
cd playground
npm install
npm run dev
# Open http://localhost:3000
```

See [playground/README.md](playground/README.md) for details.

---

## Supported Diagram Types

| Type | Keyword | Status |
|------|---------|--------|
| Flowchart | `flowchart` | Stable |
| Sequence | `sequence` | Stable |
| Architecture | `architecture` | Stable |
| State machine | `state` | Beta |
| ER diagram | `er` | Beta |
| Mind map | `mindmap` | Beta |

Browse [showcase/](showcase/) for examples. Files use complexity prefixes: `s.` (simple), `n.` (normal), `c.` (complex).

---

## Project Structure

```
drawify/
├── crates/
│   ├── drawify-core/     # Parser, AST, validation, layout, rendering
│   ├── drawify-cli/      # Command-line tool
│   ├── drawify-server/   # HTTP API service
│   ├── drawify-wasm/     # WASM bindings for the browser
│   └── drawify-eval/     # Evaluation metrics
├── docs/
│   ├── specs/            # Language and style specifications
│   ├── product/          # Vision, features, and use cases
│   └── architecture/     # Design philosophy and layout algorithms
├── showcase/             # Example diagrams by type (.dfy)
├── playground/           # React + WASM live editor
└── Cargo.toml            # Rust workspace
```

---

## Documentation

| Topic | Location |
|-------|----------|
| Language spec | [docs/specs/](docs/specs/) |
| Visual language guide | [docs/specs/visual-language/](docs/specs/visual-language/) |
| Design philosophy | [docs/architecture/design-philosophy.md](docs/architecture/design-philosophy.md) |
| Product vision | [docs/product/vision.md](docs/product/vision.md) |
| Comparison with Mermaid / PlantUML | [docs/product/comparison.md](docs/product/comparison.md) |

---

## File Extensions

| Extension | Description |
|-----------|-------------|
| `.drawify` | Full extension |
| `.dfy` | Short extension (recommended) |

---

## License

This project is licensed under the [MIT License](LICENSE).
