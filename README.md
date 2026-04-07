# clu

A minimal, blazingly fast AI agent core written in Rust.

## Overview

**clu** (Core LLM Utility) is a high-performance, memory-safe foundation for building AI agents. It features a headless core design that separates the agent logic from the UI via a well-defined bridge protocol.

## Architecture

| Crate | Responsibility |
|-------|--------------|
| `clu-ai` | LLM provider abstractions |
| `clu-core` | Agent session and state management |
| `clu-tools` | Tool implementations (read, write, edit, bash) |
| `clu-rpc` | Headless RPC host |
| `clu-tui` | Terminal frontend (Ratatui) |
| `clu-sdk` | Language bindings for external integration |
| `clu` | Meta-package combining everything |

## Quick Start

```bash
# Build the workspace
cargo build --release

# Run the TUI
cargo run --release --bin clu
```

## Design

See [DESIGN.md](./DESIGN.md) for detailed architecture and roadmap.

## License

BSD 3-Clause License
