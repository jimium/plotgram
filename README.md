# Tautcore

**Turn anything into a diagram — a diagram description language and rendering engine built for AI agents.**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org)

Tautcore is **not** a drop-in replacement for Mermaid. It is a diagram language designed from the ground up for **machine generation**: LLMs write the source, the engine handles layout, and humans read the result.

---

## Why Tautcore?

Traditional diagram tools (Mermaid, PlantUML, Graphviz) were built for humans typing by hand. AI agents need something different:

| Challenge | Legacy tools | Tautcore |
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

```tautcore
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
cargo run -p tautcore-cli -- render apps/showcase/flowchart/product.linear-chain.taut -f svg -o output.svg
```

---

## Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) 1.75 or later
- (Optional) Node.js 16+ for the playground editor

### Build

```bash
git clone https://github.com/your-org/tautcore.git
cd tautcore
cargo build --release
```

### Install CLI

```bash
cargo install --path crates/tautcore-cli
tautcore --help
```

---

## Usage

### CLI

| Command | Description |
|---------|-------------|
| `tautcore render <file>` | Parse and render a `.taut` file (`-f svg\|ascii\|png\|webp\|json`) |
| `tautcore validate <file>` | Check syntax and semantics |
| `tautcore export <file>` | Export the AST as JSON |
| `tautcore diff -o old.taut -n new.taut` | Semantic diff between two files |
| `tautcore patch <file> <patch.json>` | Apply an AST-level patch |

```bash
# Render to stdout (default format: SVG)
tautcore render examples/my-diagram.taut

# Validate and print diagnostics
tautcore validate examples/my-diagram.taut
```

### HTTP Server

```bash
cargo run -p tautcore-server
# Listens on 0.0.0.0:6080 (override with TAUTCORE_SERVER_ADDR)
```

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check |
| `/validate` | POST | Validate source (`{ "source": "..." }`) |
| `/render` | POST | Render source (`{ "source": "...", "format": "svg" }`) |

### Playground

Browser-based live editor powered by WASM:

```bash
cd apps/playground
npm install
npm run dev
# Open http://localhost:3000
```

See [apps/playground/README.md](apps/playground/README.md) for details.

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

Browse [apps/showcase/](apps/showcase/) for examples. Files use complexity prefixes: `s.` (simple), `n.` (normal), `c.` (complex).

---

## Project Structure

```
tautcore/
├── crates/
│   ├── tautcore-core/     # Parser, AST, validation, layout, rendering
│   ├── tautcore-cli/      # Command-line tool
│   ├── tautcore-server/   # HTTP API service
│   ├── tautcore-wasm/     # WASM bindings for the browser
│   └── tautcore-eval/     # Evaluation metrics
├── apps/
│   ├── showcase/          # Example diagrams by type (.taut)
│   ├── playground/        # React + WASM live editor
│   ├── website/           # Landing page
│   └── editors/           # IDE extensions (VSCode)
├── docs/
│   ├── specs/            # Language and style specifications
│   ├── product/          # Vision, features, and use cases
│   └── architecture/     # Design philosophy and layout algorithms
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
| `.tautcore` | Full extension |
| `.taut` | Short extension (recommended) |

---

## License

This project is licensed under the [MIT License](LICENSE).
