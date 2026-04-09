# clu — Engineering Plan

## Layer 1 — Foundation

- [x] Scaffold monorepo layout (`modules/` directory structure matching DESIGN.md)
- [x] Implement `core/tools` — Tool interface (`name`, `description`, `input_schema`, `call`)
- [x] Implement `core/providers` — Provider interface (`complete`)
- [x] Implement `core/storages` — Session storage interface (`append`, `history`, `clear`)
- [x] Implement `core/extensions` — Extension interface (`tools`, `on_session_start`, `on_session_end`)
- [x] Implement `core/agent` — Agentic loop (build messages → call provider → dispatch tools → repeat)

## Layer 2 — Implementations

- [ ] Implement `providers/openai-compat` — OpenAI-compatible Chat Completions provider (`base_url`, `api_key`, `model`)
- [ ] Implement `storage/memory` — In-memory ephemeral session storage
- [ ] Implement `storage/jsonl` — JSONL file-backed persistent session storage (session file path convention)
- [ ] Implement `tools/bash` — Bash shell tool (`stdout`, `stderr`, `exit_code`)
- [ ] Implement `tools/pwsh` — PowerShell tool (`stdout`, `stderr`, `exit_code`)
- [ ] Implement `tools/fs` — File system tools (`read`, `write`, `edit`, `grep`, `find`, `ls`)
- [ ] Implement `tools/node` — Persistent Node.js REPL tool (state across calls, subprocess lifecycle)

## Layer 3 — Communication Surfaces

- [ ] Implement `comms/repl` — Interactive REPL mode (prompt, streaming output, tool call display, built-in commands, shell prefixes, thinking blocks, multi-line input)
- [ ] Implement `comms/rpc` — RPC server over Unix socket / named pipe (NDJSON, `create_session`, `send_message`, `destroy_session`)

## Layer 4 — Assembly & Targets

- [ ] Implement `modules/clu` — Top-level CLI binary (flags, config loading, package wiring)
- [ ] Implement TOML configuration (providers, model aliases, defaults, storage dir)
- [ ] Implement WASM build target (C-ABI exports, exclude shell tools, host-injected tools)
