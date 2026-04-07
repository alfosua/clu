# clu Design Document: The Terminal Bridge Architecture

## 1. Vision
**clu** (Core LLM Utility) is a minimal, blazingly fast AI agent core written in Rust. It is designed to be a high-performance, memory-safe foundation for building diverse AI agents. Ported from the philosophy of `pi-coding-agent`, it focuses on modularity, extensibility, and low resource footprint.

## 2. Core Philosophy: The Terminal Bridge
The most distinctive feature of **clu** is its **Headless Core** design. Instead of a monolithic application where the agent logic and UI are tightly coupled, **clu** separates them via a well-defined **Bridge Protocol**.

### 2.1 The Headless Engine (`clu-core`)
- **State Machine**: The core manages the conversation state, tool registry, and model interaction.
- **Protocol-Agnostic**: It does not know about terminals, pixels, or sockets.
- **Commands**: Receives structured `Command` objects (e.g., `Prompt`, `Abort`, `SetModel`).
- **Events**: Emits structured `Event` objects (e.g., `Thinking(bool)`, `MessageDelta(String)`, `ToolExecution(Result)`).

### 2.2 The Bridge Protocol
The communication between the Core and the UI happens via an event-driven protocol:
- **Internal**: Uses high-performance async channels (`tokio::mpsc`).
- **External (RPC)**: Serialized to JSON-L over `stdin/stdout`, allowing any language (Python, Go, JS) to control the engine.

### 2.3 The TUI Frontend (`clu-cli`)
- A standalone "view" built with **Ratatui**.
- Subscribes to the Core's events and renders them.
- Forwards user input as commands back to the Core.
- This separation allows for swapping the TUI for a Web UI, Desktop UI, or IDE plugin without changing the agent's brain.

---

## 3. Modular Crate Structure (The Workspace)

| Component | Responsibility |
| :--- | :--- |
| **`clu-ai`** | Abstractions for LLM providers (OpenAI-compatible first, Anthropic, etc.). Handles API requests and streaming. |
| **`clu-core`** | The "Brain". The core library that manages sessions, tools, and the bridge logic. |
| **`clu-tools`** | Implementation of standard tools (`read`, `write`, `edit`, `bash`). |
| **`clu-rpc`** | The Headless Host. A binary that runs `clu-core` and exposes the **RPC Protocol**. |
| **`clu-tui`** | The Terminal Frontend. A standalone binary that connects to a `clu-rpc` to provide the UI. |
| **`clu-sdk`** | A lightweight library for other languages to easily talk to the `clu-rpc`. |
| **`clu`** | The final meta-package that boxes everything together into a single, easy-to-install binary. |

---

## 4. Key Features

### 4.1 Modularity & Extensibility
- **Internal Traits**: Core components (Tools, Providers) are defined as Rust Traits for compile-time modularity.
- **Dynamic Plugins (Future)**: Support for **WebAssembly (Wasm)** based plugins, allowing users to write extensions in any language that compile to Wasm (Rust, Go, Zig) with near-native performance and perfect sandboxing.

### 4.2 Portability (The SDK Strategy)
- **RPC Mode**: The primary way to integrate with other languages. `clu --mode rpc` starts the headless engine.
- **FFI**: Exporting C-compatible headers using `uniffi` or `cbindgen` for direct language bindings.

### 4.3 High Performance
- **Zero-Copy**: Efficient handling of large file contents using Rust's memory management (`Cow`, `Arc<str>`).
- **Statically Linked**: A single, small binary with minimal dependencies.
- **Async First**: Fully asynchronous I/O using `tokio`.

---

## 5. Development Roadmap

1. **Phase 1: The Protocol.** Define the `Event` and `Command` schemas.
2. **Phase 2: The Brain.** Build the basic `AgentSession` loop in `clu-core`.
3. **Phase 3: The Connection.** Implement the first LLM provider in `clu-ai` (OpenAI-compatible endpoints, e.g., for Ollama).
4. **Phase 4: The Hands.** Implement the basic `read` and `bash` tools.
5. **Phase 5: The Interface.** Build the first iteration of the Ratatui-based TUI.
6. **Phase 6: The Bridge.** Finalize the RPC mode for external language support.
7. **Phase 7: The Package.** Create the final `clu` meta-package to box everything together for quick installation.
