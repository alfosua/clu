# `clu` CLI Specification

The `clu` binary is the single user-facing entry point for the Core LLM Utility.
It dispatches to the TUI, the RPC server, or the non-interactive "quick" mode
based on the flags and positional arguments it receives.

## Synopsis

```
clu [OPTIONS] [MESSAGE]...
```

`MESSAGE` positional arguments are user prompts. When multiple are given, they
are delivered to the agent one after another (the first as the initial prompt,
the rest queued as follow-ups).

## Modes

`clu` runs in one of three modes, determined by the following precedence:

1. `--rpc`              → **RPC mode** (JSONL over stdin/stdout; see `clu-rpc`)
2. `--quick` / `-q`     → **Quick mode** (non-interactive: run once, print, exit)
3. *(default)*          → **TUI mode** (interactive terminal UI; see `clu-tui`)

Only one mode flag may be used at a time. Using both `--rpc` and `--quick`
is an error.

## Examples

```bash
# Runs clu tui with a new agent session in the current working directory.
clu

# Runs clu tui with the last agent session opened in the current working directory.
clu --continue
clu -c

# Runs clu special tui to select a session available in the current working directory.
clu --resume
clu -r

# Runs clu tui with an initial prompt in a new agent session.
clu "Fix this issue"

# Runs clu tui with a sequence of messages, one queued after the other.
clu "Solve this problem" "Solve the other problem"

# Runs clu as an RPC server using stdin/stdout.
clu --rpc

# Runs clu in a non-interactive mode (process and exit).
clu --quick "Tell me how to fix the world"
clu -q "..."

# Runs clu with a specific session file (JSONL path).
clu --session <path/to/session.jsonl> ...

# Runs clu with a specific session storage directory.
clu --storage <path/to/session/storage> ...

# Runs clu with a specific model (model ID, provider inline).
clu --model <provider/model:tag> ...

# Runs clu with models whose IDs match a glob pattern.
clu --model <provider/*> ...

# Runs clu with a provider and model.
clu --provider <provider> --model <model:tag> ...

# Runs clu with a set of available models (comma-separated) for cycling.
clu --models <model1>,<model2>,<model3> ...

# Runs clu with a set of available providers (comma-separated).
clu --providers <provider1>,<provider2>,<provider3>

# Runs clu with a set of available tools (comma-separated).
clu --tools <tool1>,<tool2>,<tool3> ...
# Built-in tools: read, grep, find, ls, write, edit, bash
# Read-only mode example:
clu --tools read,grep,find,ls ...

# Runs clu with a thinking level.
clu --thinking <level> ...
# Levels: off, minimal, low, medium, high, xhigh
```

## Options

### Mode selection

| Flag | Description |
|---|---|
| *(none)* | Interactive TUI mode (default). |
| `--rpc` | RPC server over stdin/stdout (JSONL framing, same protocol as `clu-rpc`). |
| `-q`, `--quick <PROMPT>` | Non-interactive: send the given prompt, stream output to stdout, exit. In `--quick` mode the prompt may be given either as the flag value or as positional `MESSAGE` arguments. |

### Session options

| Flag | Description |
|---|---|
| `-c`, `--continue` | Continue the most recent session in the current working directory. |
| `-r`, `--resume` | Open the TUI session picker to select an existing session from the current working directory. |
| `--session <PATH>` | Load a specific session file (`.jsonl`). Takes precedence over `--continue`/`--resume`. |
| `--storage <PATH>` | Use a custom directory for session storage (defaults to `~/.clu/sessions`). |

### Model options

| Flag | Description |
|---|---|
| `--provider <NAME>` | Preferred provider (e.g. `openai`, `anthropic`, `openai-compatible`). |
| `--model <PATTERN>` | Active model. Accepts a bare id (`gpt-4o`), a `provider/id` form (`openai/gpt-4o`), an optional `:thinking` suffix (`sonnet:high`), or a glob (`openai/*`). |
| `--models <LIST>` | Comma-separated set of models available for cycling (Ctrl+P in TUI). |
| `--providers <LIST>` | Comma-separated set of providers the client is allowed to use. |
| `--thinking <LEVEL>` | Reasoning level: `off`, `minimal`, `low`, `medium`, `high`, `xhigh`. |

### Tool options

| Flag | Description |
|---|---|
| `--tools <LIST>` | Comma-separated set of built-in tools to enable. Built-ins: `read`, `grep`, `find`, `ls`, `write`, `edit`, `bash`. |

### General

| Flag | Description |
|---|---|
| `-h`, `--help` | Print help and exit. |
| `-V`, `--version` | Print version and exit. |

## Argument rules

1. `--rpc` is mutually exclusive with `--quick`, `--continue`, `--resume`, and positional messages.
2. `--continue` and `--resume` are mutually exclusive.
3. `--session` overrides `--continue`/`--resume`.
4. In `--quick` mode, at least one prompt must be provided (either via the
   flag value or positional `MESSAGE` arguments). Multiple messages are joined
   in order: first as the initial prompt, the rest as queued follow-ups.
5. `--provider` may be omitted when `--model` is given in the `provider/id`
   form. If both are given and disagree, the value of `--provider` wins.
6. `--model` accepting a glob (`provider/*`) implies the set of matching
   models should be enabled; behaves like `--models` matched by the pattern.

## Exit codes

| Code | Meaning |
|---|---|
| `0` | Success. |
| `1` | Generic runtime error. |
| `2` | Invalid arguments / usage. |
| `130` | Interrupted (Ctrl+C). |
