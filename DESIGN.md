# clu — Core LLM Utility: Design Document

## Vision

**clu** is a portable, extensible agentic AI runner that bridges natural language to tools. Unlike opinionated coding agents, clu is a general-purpose substrate: any software can embed it, any tool can plug into it, and any LLM-compatible provider can power it.

The guiding principle is **minimal core, maximum extensibility** — ship just enough to be useful by default, then get out of the way.

---

## Goals

- **Portable** — Written in Zig, targeting native binaries and WebAssembly (WASM) for direct in-browser use.
- **Provider-agnostic** — Supports any OpenAI-compatible Chat Completions API (OpenAI, Ollama, custom deployments).
- **Embeddable** — Acts as a library/bridge for IDEs, custom TUIs, and other host software.
- **Extensible** — Tools, providers, and extensions are discrete modules with a stable interface.
- **Minimal by default** — The only built-in tool is a shell runner; everything else is opt-in.
- **Interactive** — A first-class REPL mode for direct human use, modeled after `python`, `node`, or a shell.

---

## Architecture

clu is organized as a loose monorepo of modules grouped by concern. No module is a hard dependency of another unless strictly necessary. Modules are composed at the top-level CLI layer.

```text
clu/
├── modules/
│   │
│   ├── core/                   # Engine: agent loop and interfaces
│   │   ├── agent/              #   Agentic loop and turn orchestration
│   │   ├── tools/              #   Tool interface definition
│   │   ├── providers/          #   Provider interface definition
│   │   ├── storages/           #   Session storage interface definition
│   │   └── extensions/         #   Extension interface definition
│   │
│   ├── providers/              # LLM provider implementations
│   │   └── openai-compat/      #   OpenAI-compatible Chat Completions
│   │
│   ├── tools/                  # Tool implementations
│   │   ├── bash/               #   Bash shell tool
│   │   ├── pwsh/               #   PowerShell tool
│   │   ├── fs/                 #   File system tools (read, write, edit, grep, find, ls)
│   │   └── node/               #   Node.js REPL tool
│   │
│   ├── storage/                # Session storage backends
│   │   ├── memory/             #   In-memory (default, ephemeral)
│   │   └── jsonl/              #   JSONL file storage (persistent, per-directory)
│   │
│   ├── comms/                  # Communication surfaces
│   │   ├── repl/               #   Interactive REPL mode
│   │   └── rpc/                #   RPC server for IDE and TUI bridge integrations
│   │
│   └── clu/                    # Top-level CLI: composes all modules into a distributable binary
│
├── DESIGN.md
└── README.md
```

Each package is independently buildable. The `clu` top-level package is the only one that wires everything together.

---

## Core Packages (`modules/core/`)

The engine of clu. Implements the agentic loop and defines stable contracts that all other modules depend on.

### Agent (`core/agent`)

Implements the agentic loop.

```text
user input
    → build message list
    → call provider (LLM)
    → receive response
        → if text: emit to output
        → if tool_call: dispatch to registered tool handler
            → append tool result to message list
            → repeat from "call provider"
    → done
```

The loop runs until the model produces a natural stop or a configured step limit is hit.

### Interface: Tool (`core/tools`)

Defines what a tool is. Any package that wants to expose a tool implements this contract.

- **name** — unique string identifier (e.g. `bash`)
- **description** — natural language description sent to the model
- **input_schema** — JSON Schema object describing the tool's parameters
- **call(input) → output** — the execution function; input and output are JSON values

### Interface: Provider (`core/providers`)

Defines what an LLM provider is.

- **complete(messages, tools) → response** — sends a message list and registered tool schemas; returns either a text reply or a list of tool calls to dispatch

### Interface: Session Storage (`core/storages`)

Defines how message history is persisted within and optionally across sessions.

- **append(message)** — add a message to history
- **history() → []message** — retrieve full history
- **clear()** — reset history

The interface only defines the contract. Implementations live in `modules/storage/`.

### Interface: Extension (`core/extensions`)

Defines what an extension is — a bundle of tools and/or lifecycle hooks that can be registered into a session. Extensions are the composition unit for adding capabilities beyond what a bare tool provides (e.g. setup, teardown, config injection).

- **tools() → []Tool** — tools this extension contributes
- **on_session_start(session)** — called when a session begins
- **on_session_end(session)** — called when a session ends

---

## Providers (`modules/providers/`)

### OpenAI-Compatible (`providers/openai-compat`)

Implements the **Provider** interface against the OpenAI Chat Completions API (`/v1/chat/completions`). Covers OpenAI, Ollama, and any compatible custom API through configuration alone.

| Field | Description |
| --- | --- |
| `base_url` | Base URL of the API (e.g. `https://api.openai.com`, `http://localhost:11434`) |
| `api_key` | Bearer token sent as `Authorization: Bearer <key>` |
| `model` | Model identifier (e.g. `gpt-4o`, `llama3`) |

---

## Storage (`modules/storage/`)

Session storage backends implement the `core/storages` contract. The active backend is selected by CLI flags or configuration.

### In-Memory (`storage/memory`)

The default. History lives only for the duration of the process. Used when no `--session` or `--storage` flag is given and no default storage directory is configured.

### JSONL (`storage/jsonl`)

Persists message history to a `.jsonl` file — one JSON object per line, one line per message. This is the format used for named session files.

When a storage directory is active (via `--storage` or the default `~/.local/share/clu/sessions/`), each session is stored as a separate JSONL file named by session ID and scoped to the working directory. This enables `--continue` (reopen the last session) and `--resume` (pick from available sessions).

Session file path convention: `<storage-dir>/<cwd-hash>/<session-id>.jsonl`

---

## Tools (`modules/tools/`)

Tools are the unit of capability. The core ships no tools; tool modules do.

One shell tool is registered per session based on the host OS or explicit configuration. Each tool is self-contained — its own name, description, and schema. The output shape is shared by convention, not enforcement.

### Bash (`tools/bash`)

Available on Unix and Windows (via WSL or Git Bash).

**Tool: `bash`**

```json
{
  "name": "bash",
  "description": "Execute a Bash command and return its output.",
  "input_schema": {
    "type": "object",
    "properties": {
      "command": { "type": "string", "description": "The Bash command to run." }
    },
    "required": ["command"]
  }
}
```

- Executes the command string in a `bash` subprocess.
- Captures stdout and stderr separately.
- Returns exit code alongside output.

### PowerShell (`tools/pwsh`)

Available on Windows natively; also available cross-platform via `pwsh`.

**Tool: `pwsh`**

```json
{
  "name": "pwsh",
  "description": "Execute a PowerShell command and return its output.",
  "input_schema": {
    "type": "object",
    "properties": {
      "command": { "type": "string", "description": "The PowerShell command to run." }
    },
    "required": ["command"]
  }
}
```

- Executes the command string via `pwsh -Command`.
- Captures stdout and stderr separately.
- Returns exit code alongside output.

### Shared Output Shape (by convention)

Both tools return the same JSON structure:

```json
{
  "stdout": "...",
  "stderr": "...",
  "exit_code": 0
}
```

### File System (`tools/fs`)

A suite of tools for reading, writing, and searching files. Scoped to the working directory by default.

**Tool: `read`** — read the contents of a file.

```json
{
  "name": "read",
  "description": "Read the contents of a file and return them as a string.",
  "input_schema": {
    "type": "object",
    "properties": {
      "path": { "type": "string", "description": "Path to the file." },
      "offset": { "type": "integer", "description": "Line number to start reading from (1-indexed, optional)." },
      "limit": { "type": "integer", "description": "Maximum number of lines to return (optional)." }
    },
    "required": ["path"]
  }
}
```

**Tool: `write`** — write or overwrite a file.

```json
{
  "name": "write",
  "description": "Write content to a file, overwriting it if it already exists.",
  "input_schema": {
    "type": "object",
    "properties": {
      "path": { "type": "string", "description": "Path to the file." },
      "content": { "type": "string", "description": "Content to write." }
    },
    "required": ["path", "content"]
  }
}
```

**Tool: `edit`** — replace an exact string within a file.

```json
{
  "name": "edit",
  "description": "Replace an exact occurrence of old_string with new_string in a file.",
  "input_schema": {
    "type": "object",
    "properties": {
      "path": { "type": "string", "description": "Path to the file." },
      "old_string": { "type": "string", "description": "Exact string to replace." },
      "new_string": { "type": "string", "description": "Replacement string." }
    },
    "required": ["path", "old_string", "new_string"]
  }
}
```

**Tool: `grep`** — search file contents by pattern.

```json
{
  "name": "grep",
  "description": "Search for a regex pattern in files and return matching lines with file paths and line numbers.",
  "input_schema": {
    "type": "object",
    "properties": {
      "pattern": { "type": "string", "description": "Regex pattern to search for." },
      "path": { "type": "string", "description": "File or directory to search in (defaults to working directory)." },
      "glob": { "type": "string", "description": "Glob pattern to filter files (e.g. '*.ts')." }
    },
    "required": ["pattern"]
  }
}
```

**Tool: `find`** — locate files by name pattern.

```json
{
  "name": "find",
  "description": "Find files matching a glob pattern under a directory.",
  "input_schema": {
    "type": "object",
    "properties": {
      "pattern": { "type": "string", "description": "Glob pattern to match (e.g. '**/*.zig')." },
      "path": { "type": "string", "description": "Directory to search in (defaults to working directory)." }
    },
    "required": ["pattern"]
  }
}
```

**Tool: `ls`** — list directory contents.

```json
{
  "name": "ls",
  "description": "List the contents of a directory.",
  "input_schema": {
    "type": "object",
    "properties": {
      "path": { "type": "string", "description": "Directory to list (defaults to working directory)." }
    },
    "required": []
  }
}
```

### Node.js (`tools/node`)

Executes JavaScript in a persistent Node.js REPL subprocess. State (variables, imports) is preserved across calls within the same session.

**Tool: `node`**

```json
{
  "name": "node",
  "description": "Execute JavaScript in a persistent Node.js REPL and return the result.",
  "input_schema": {
    "type": "object",
    "properties": {
      "code": { "type": "string", "description": "JavaScript to evaluate." }
    },
    "required": ["code"]
  }
}
```

- Runs code in a long-lived `node` subprocess; state persists across tool calls within a session.
- Returns the evaluated result, stdout, and stderr.
- The subprocess is terminated when the session ends.

### Future Tools (not in scope now)

- HTTP fetch
- Code execution sandboxes
- Browser automation

---

## Comms (`modules/comms/`)

Communication surfaces that expose a running clu session to humans or external software.

### REPL (`comms/repl`)

A minimal interactive terminal mode. No TUI library — raw terminal I/O, modeled after `python -i` or `node`.

#### Session Start

```text
clu
Welcome to clu. Type /help for commands, Ctrl+D to exit.
[bash | gpt-4o | openai]
>>>
```

The status line after the welcome message shows the active shell tool, model, and provider alias. It updates if configuration changes mid-session.

#### Normal Interaction

Tool calls are printed inline as they are dispatched. While a tool is running, the command is shown. Once it returns, the result collapses to a single summary line by default:

```text
>>> what files changed in the last commit?
  ✔ bash: git diff --name-only HEAD~1 (2 lines)

The last commit modified two files:
- src/main.zig
- modules/core/agent.zig
>>>
```

The result line shows the tool name, the command, and a brief output summary (line count, exit code if non-zero). Expanding reveals the full output:

```text
>>> what files changed in the last commit?
  ✔ bash: git diff --name-only HEAD~1
    src/main.zig
    modules/core/agent.zig

The last commit modified two files:
- src/main.zig
- modules/core/agent.zig
>>>
```

When a tool call fails (non-zero exit), the summary line signals the error:

```text
>>> run the tests
  ✘ bash: cargo test (exit 101, 34 lines)

The test run failed. Here's what went wrong...
>>>
```

#### Multi-line Input

Trailing `\` continues input on the next line. A blank line after continuation finishes the block.

```text
>>> explain the difference between \
... bash and pwsh \
... in one sentence
```

#### Built-in Commands

| Command | Effect |
| --- | --- |
| `/reset` | Clear session history, start fresh |
| `/history` | Print full message history |
| `/tools` | List registered tools |
| `/model <id>` | Switch model for this session |
| `/models` | List available models |
| `/thinking [level]` | Get or set thinking depth (`off` / `minimal` / `low` / `medium` / `high` / `xhigh`) |
| `/help` | Print command reference |
| `/quit` or `/exit` | Quit |
| Ctrl+D | Quit |
| Ctrl+C Ctrl+C | Quit (double Ctrl+C within 1 second) |

#### Shell Prefixes

Lines prefixed with `!` run a shell command directly and inject both the command and its output into the conversation as a user message, so the model has the result as context.

```text
>>> ! git log --oneline -5
f3a91c2 fix auth token refresh
8d02e44 add pwsh tool
...

(output added to context)
>>>
```

Lines prefixed with `!!` run the command and print the output, but do not inject anything into the conversation. Useful for quick inspection without polluting context.

```text
>>> !! ls -la
total 48
drwxr-xr-x  6 user user 4096 Apr  8 12:00 .
...

(output not added to context)
>>>
```

#### Thinking Blocks

When the model uses extended thinking (enabled via `--thinking`), the reasoning is shown collapsed by default, expandable on demand.

```text
>>> design a migration plan for adding soft deletes to the users table
  ▸ thinking (42 tokens)

Here's a migration plan for soft deletes on the `users` table:
1. Add a `deleted_at` column (nullable timestamp)...
>>>
```

The thinking prefix shows token count so the user can gauge depth without reading the full trace. Expanding shows the raw reasoning:

```text
>>> design a migration plan for adding soft deletes to the users table
  ▾ thinking (42 tokens)
    I need to consider backward compatibility — existing queries that filter
    users will need a `WHERE deleted_at IS NULL` guard. The safest approach
    is an additive migration: add the column, backfill NULL, then update
    application queries before enforcing a NOT NULL constraint...

Here's a migration plan for soft deletes on the `users` table:
1. Add a `deleted_at` column (nullable timestamp)...
>>>
```

#### Output Streaming

Agent text output is streamed token-by-token as it arrives from the provider. Tool dispatch interrupts the stream, prints the tool call line, then resumes streaming the model's continuation.

---

### RPC (`comms/rpc`)

Exposes a running clu agent over a local socket so external software (IDEs, TUIs, custom frontends) can drive it programmatically. The transport is a Unix domain socket (or named pipe on Windows). The wire format is newline-delimited JSON (NDJSON).

Each message is a JSON object on a single line, terminated by `\n`.

#### Client → Server Messages

**`create_session`** — create a new agent session

```json
{
  "id": "req-1",
  "method": "create_session",
  "params": {
    "model": "gpt-4o",
    "system_prompt": "You are a helpful assistant.",
    "tools": ["bash"]
  }
}
```

Response:

```json
{
  "id": "req-1",
  "result": {
    "session_id": "sess-abc123"
  }
}
```

---

**`send_message`** — send a user message to a session and stream back the agent's response

```json
{
  "id": "req-2",
  "method": "send_message",
  "params": {
    "session_id": "sess-abc123",
    "content": "What is in the current directory?"
  }
}
```

The server streams back a sequence of event objects, all sharing the same `id`, until a final `done` event:

```json
{ "id": "req-2", "event": "text_delta",   "data": { "delta": "Let me check" } }
{ "id": "req-2", "event": "text_delta",   "data": { "delta": " that for you." } }
{ "id": "req-2", "event": "tool_start",   "data": { "tool": "bash", "input": { "command": "ls" } } }
{ "id": "req-2", "event": "tool_end",     "data": { "tool": "bash", "output": { "stdout": "main.zig\nREADME.md", "stderr": "", "exit_code": 0 } } }
{ "id": "req-2", "event": "text_delta",   "data": { "delta": "The directory contains: main.zig, README.md." } }
{ "id": "req-2", "event": "done",         "data": {} }
```

---

**`destroy_session`** — tear down a session and free its resources

```json
{
  "id": "req-3",
  "method": "destroy_session",
  "params": {
    "session_id": "sess-abc123"
  }
}
```

Response:

```json
{
  "id": "req-3",
  "result": {}
}
```

---

#### Error Shape

Any response (including mid-stream) can carry an `error` field instead of `result`/`event`:

```json
{
  "id": "req-2",
  "error": {
    "code": "provider_error",
    "message": "upstream API returned 429 Too Many Requests"
  }
}
```

#### Event Types Summary

| Event | When emitted |
| --- | --- |
| `text_delta` | Model is streaming a text token |
| `tool_start` | A tool call was dispatched |
| `tool_end` | A tool call returned |
| `done` | Agent turn is complete, no more events for this request |

---

## CLI (`modules/clu`)

The top-level binary. Composes core, providers, tools, storage, and comms into a single distributable. There are no subcommands — all behavior is controlled through flags and optional positional prompt arguments.

### Invocation

```text
clu [flags] [prompt...]
```

When invoked without prompts, clu opens the interactive REPL. When prompts are provided, they are queued as initial messages and the REPL opens with them pre-loaded. With `--quick`, the REPL is skipped entirely and clu runs to completion and exits.

### Session Flags

| Flag | Short | Description |
| --- | --- | --- |
| `--continue` | `-c` | Reopen the last session for the current working directory |
| `--resume` | `-r` | Open a session picker for sessions in the current working directory |
| `--session <path>` | | Load a specific session from a JSONL file |
| `--storage <dir>` | | Override the session storage directory |

### Mode Flags

| Flag | Short | Description |
| --- | --- | --- |
| `--quick <prompt>` | `-q` | Non-interactive: run the agent to completion and exit, no REPL |
| `--rpc` | | Start in RPC mode: read requests from stdin, write events to stdout |

### Provider & Model Flags

| Flag | Description |
| --- | --- |
| `--provider <name>` | Select a named provider from config |
| `--providers <p1>,<p2>,...` | Activate a set of providers, cycling is available in REPL |
| `--model <provider/model:tag>` | Select a specific model; provider prefix is optional if unambiguous |
| `--models <m1>,<m2>,...` | Activate a set of models, cycling is available in REPL |

The `--model` flag accepts a glob pattern. `provider/*` selects all models from that provider; the REPL lets the user cycle through matches.

### Tool Flags

| Flag | Description |
| --- | --- |
| `--tools <t1>,<t2>,...` | Restrict the active tool set to the listed tools |

Built-in tools (registered in the top-level `clu` package):

| Tool | Description |
| --- | --- |
| `bash` | Run a Bash command |
| `pwsh` | Run a PowerShell command |
| `read` | Read a file |
| `write` | Write a file |
| `edit` | Replace a string in a file |
| `grep` | Search file contents by regex |
| `find` | Find files by glob pattern |
| `ls` | List directory contents |
| `node` | Execute JavaScript in a persistent Node.js REPL |

Read-only subsets are easy to compose with `--tools`:

```sh
# read-only shell inspection
clu --tools bash "what dependencies does this project have?"
```

### Inference Flags

| Flag | Description |
| --- | --- |
| `--thinking <level>` | Set the model's thinking depth |
| `--max-steps <n>` | Limit the number of agent loop iterations |

Thinking levels: `off`, `minimal`, `low`, `medium`, `high`, `xhigh`.

### Other Flags

| Flag | Description |
| --- | --- |
| `--system <prompt>` | Override the system prompt for this session |
| `--config <path>` | Config file path (default: `~/.clu/config.toml` on Unix, `%USERPROFILE%\.clu\config.toml` on Windows) |

### Examples

```sh
# Open REPL with a new session in the current directory
clu

# Resume the last session in the current directory
clu --continue

# Pick a session from the current directory interactively
clu --resume

# Open REPL with an initial prompt already queued
clu "Fix the failing tests"

# Queue multiple prompts to run back to back
clu "Refactor the auth module" "Write tests for the changes"

# Non-interactive one-shot, exits when done
clu -q "Summarize the recent git log"

# Use a specific session file
clu --session ./sessions/my-debug.jsonl

# Use a custom storage directory
clu --storage ./sessions

# RPC mode (for IDE integrations)
clu --rpc

# Specific model from a named provider
clu --model openai/gpt-4o

# All models from a provider, cycle between them in REPL
clu --model "ollama/*"

# Explicit provider + model
clu --provider ollama --model llama3.1:8b

# Multiple models to cycle between
clu --models openai/gpt-4o,ollama/llama3.1:8b

# Read-only session: only allow inspection tools
clu --tools bash "what is the structure of this project?"

# Enable deep thinking
clu --thinking high "Design a migration plan for this schema"

# Connect to a local Ollama instance
clu --provider ollama --model llama3.1 "explain this Makefile"
```

---

## WASM Target

clu's core (agent loop + provider client + tool dispatch) compiles to a WASM module. The WASM build:
- Exposes a minimal C-ABI for creating sessions, sending messages, and receiving streamed output
- Excludes shell tools (subprocess execution is not available in browser WASM sandboxes)
- Relies on the host (browser JS) to inject tool implementations via the Tool interface

This makes clu usable as an in-browser AI runner — the host page registers tools (DOM manipulation, fetch, storage) and clu drives the loop.

---

## Configuration

clu is configured via a file (`~/.clu/config.toml` on Unix, `%USERPROFILE%\.clu\config.toml` on Windows) and environment variables, with CLI flags taking highest precedence.

### Providers

Each provider is declared under `[provider.<name>]` and referenced by that name via `--provider <name>`.

```toml
[provider.openai]
base_url = "https://api.openai.com"
api_key  = "sk-..."

[provider.ollama]
base_url = "http://localhost:11434"
api_key  = "ollama"
```

### Models

Models are declared under `[provider.<name>.models.<alias>]`. The alias is the short name used in `--model` and `--models` flags. When you only need to map an alias to an ID, a string shorthand is accepted:

```toml
[provider.openai]
models.mini = "gpt-4o-mini"
```

When you need to declare capabilities or a display name, expand to a subtable:

```toml
[provider.openai.models.gpt4o]
model_id     = "gpt-4o"
display_name = "GPT-4o"
has_thinking = false
has_audio    = false

[provider.github_copilot.models.sonnet]
model_id     = "claude-sonnet-4-6"
display_name = "Sonnet (Copilot)"
has_audio    = false

[provider.ollama.models.sonnet]
model_id     = "llama3.1:8b"
display_name = "Sonnet (Local)"
has_vision   = false
has_thinking = false
has_audio    = false
```

The same alias (`sonnet`) can exist under multiple providers with no conflict — model aliases are always provider-scoped. Resolution:

- `--model sonnet` → resolves against the default provider's models
- `--provider ollama --model sonnet` → unambiguous
- `--model ollama/sonnet` → inline syntax, always bypasses alias lookup

#### Model Fields

| Field | Type | Required | Default | Description |
| --- | --- | --- | --- | --- |
| `model_id` | string | yes | — | Actual model ID sent to the provider API |
| `display_name` | string | no | same as alias | Label shown in REPL and logs |
| `has_tools` | bool | no | `true` | Whether the model supports tool/function calling |
| `has_vision` | bool | no | `true` | Whether the model supports image input |
| `has_thinking` | bool | no | `true` | Whether the model supports extended thinking |
| `has_audio` | bool | no | `true` | Whether the model supports audio input/output |

Capability flags default to `true` so minimal declarations work out of the box. Set a flag to `false` explicitly to disable the capability — clu will then omit tool schemas, hide vision affordances in the REPL, etc. for that model.

### Defaults and Storage

```toml
[defaults]
provider = "openai"
model    = "gpt4o"
tools    = ["bash"]

[storage]
directory = "~/.local/share/clu/sessions"
```

The `model` field under `[defaults]` is an alias name (resolved against the default provider) or an inline `provider/model_id` reference.

### Environment Variables

| Variable | Overrides |
| --- | --- |
| `CLU_PROVIDER` | `[defaults] provider` |
| `CLU_MODEL` | `[defaults] model` |
| `CLU_API_KEY` | `[provider.<default>] api_key` |
| `CLU_BASE_URL` | `[provider.<default>] base_url` |

Precedence (highest to lowest): CLI flags → environment variables → config file → built-in defaults.

---

## Non-Goals (for now)

- Built-in RAG or vector search
- Multi-agent orchestration
- GUI
- Cloud sync or accounts
- Opinionated project-type detection (no "coding agent" magic)
