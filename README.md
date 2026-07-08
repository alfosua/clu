# clu — Core LLM Utility

**An AI agent loop at the operating system level.**

Most agent frameworks are libraries: the loop, the model client, and the tools all live inside one process, in one language, behind one plugin API. `clu` takes the Unix approach instead — **every part of the agent loop is a separate program** that communicates over stdin/stdout with newline-delimited JSON:

- the **loop** is a process,
- the **inference provider** is a process,
- **every tool** is a process,
- every **input/output middleware** is a process.

Any executable that honors the (tiny) contract can replace any built-in piece. Want a tool written in Python, a provider in Rust, or a one-line shell script as middleware? Just put it on your `PATH`.

Built in **pure Go with zero third-party dependencies** — the whole system is ~1,100 lines across 10 binaries and 3 shared packages.

```sh
export ANTHROPIC_API_KEY=...
clu "Summarize the TODOs in this repo and write them to TODO.md"
```

## Why this design?

| Conventional framework | clu |
|---|---|
| Tools are plugins in the framework's language | Tools are programs in **any** language |
| Swapping the model client means changing code | Swapping the provider is a CLI flag |
| The agent loop is fixed | The loop itself is a replaceable hook |
| Extension requires learning an SDK | Extension requires reading stdin and writing stdout |

The pipe is the plugin API. That buys process isolation (a crashing tool can't take down the loop), composability (`--hook-shell` lets a hook be a shell pipeline), and testability — every component can be run and debugged by hand:

```sh
$ clu-tool-read --describe
{"name":"read","description":"Read the contents of a file at the given path.", ...}

$ echo '{"path":"go.mod"}' | clu-tool-read
module github.com/alfosua/clu
```

## Architecture

```
                 ┌────────────────────── clu (orchestrator) ───────────────────────┐
                 │                                                                 │
 user prompt ──▶ │  input middlewares ─▶ loop hook ─▶ output middlewares ─▶ stdout │
                 │   (clu-input, ...)        │         (clu-output, ...)           │
                 └─────────────────────────┬─┴───────────────────────────────────--┘
                                           │  ndjson envelopes over stdin/stdout
                           ┌───────────────┴───────────────┐
                           ▼                               ▼
                    provider hook                     tool hooks
               (clu-provider-anthropic,        (clu-tool-read, -write,
                clu-provider-openai, ...)       -ls, -bash, yours...)
```

One conversational turn:

1. `clu` runs the prompt through each **input middleware** (plain text in, plain text out).
2. It sends a `{"type":"user","text":...}` envelope to the **loop** hook's stdin.
3. The loop asks each **tool** hook to `--describe` itself, then drives the **provider** hook — one `ProviderRequest` JSON in, one `ProviderResponse` JSON out — executing tool calls as child processes until the model stops asking for tools.
4. Text envelopes flow back through the **output middlewares** to your terminal; a `done` envelope ends the turn.

The wire format lives in [`internal/protocol`](internal/protocol/protocol.go) and is deliberately transport-agnostic — shared-memory transport is a planned optimization. See [DESIGN.md](DESIGN.md) for the original design document.

## Install

Requires Go 1.26+.

```sh
git clone https://github.com/alfosua/clu
cd clu
make install   # go install ./cmd/...
```

All binaries land in `$(go env GOPATH)/bin`; make sure that's on your `PATH` so `clu` can find the default hooks.

## Usage

```sh
# One-shot prompt (defaults: Anthropic provider, read/write/ls/bash tools)
clu "Create the perfect system"

# Pipe a prompt in
cat PROMPT.md | clu

# Interactive REPL
clu -r

# Any OpenAI-compatible endpoint (e.g. a local model)
clu -p openai -m qwen3 -u http://localhost:8080/v1/chat/completions "hi"

# Restrict the toolset, tweak the system prompt
clu -t read -t ls -a "Never modify files." "Audit this directory"
```

Hook names support shorthand: `-p openai` expands to `-p clu-provider-openai`, `-t read` to `-t clu-tool-read`, and so on (prefixes: `clu-loop-`, `clu-provider-`, `clu-tool-`, `clu-input-`, `clu-output-`). The prefixed form wins when it exists on `PATH`; use a path (e.g. `./ls`) to force a same-named program. Shorthand is skipped for specs containing spaces or `/`, or when `--hook-shell` is set.

Run `clu --help` for all flags. Two deviations from the design draft avoid short-flag collisions: `--version` is `-V` and `--hook-shell` has no short form.

## Write a tool in any language

A tool hook is any executable that (a) prints its spec when called with `--describe`, and (b) otherwise reads JSON input on stdin and writes its result to stdout, exiting non-zero on error. Here is a complete weather tool in bash:

```sh
#!/usr/bin/env bash
# ~/bin/clu-tool-weather
if [ "$1" = "--describe" ]; then
  cat <<'EOF'
{"name":"weather","description":"Get the current weather for a city.",
 "input_schema":{"type":"object",
   "properties":{"city":{"type":"string"}},"required":["city"]}}
EOF
  exit 0
fi
city=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["city"])')
curl -s "https://wttr.in/${city}?format=3"
```

```sh
chmod +x ~/bin/clu-tool-weather
clu -t weather -t bash "Is it warmer in Oslo or in Madrid right now?"
```

No SDK, no recompile, no registration — the loop discovers the schema via `--describe` and the model can call it immediately.

## Hook contracts

| Hook | Contract |
|---|---|
| **Loop** | Reads `{"type":"user","text":...}` envelopes on stdin; emits `text`, `tool_use`, `tool_result`, `error`, and `done` envelopes on stdout. Configuration arrives via `CLU_*` environment variables. |
| **Provider** | Reads one `ProviderRequest` JSON on stdin; writes one `ProviderResponse` JSON on stdout. |
| **Tool** | `--describe` prints its `ToolSpec` (name, description, JSON schema); otherwise reads JSON input on stdin, writes its result to stdout, exits non-zero on error. |
| **Input/output middleware** | Plain text filter: reads stdin, writes stdout. |

With `--hook-shell <shell>`, hook specs become shell expressions instead of direct executables — so a middleware can be `-o 'tee session.log'` and a tool can be an ad-hoc pipeline.

## Project layout

Go monorepo: one module, one binary per `cmd/` directory, shared code under `internal/`. No external dependencies.

```
cmd/
  clu/                     orchestrator CLI (arg parsing, middlewares, loop lifecycle)
  clu-loop/                default single-threaded agent loop hook
  clu-provider-anthropic/  provider hook for Anthropic's Messages API
  clu-provider-openai/     provider hook for OpenAI-compatible chat APIs
  clu-tool-read/           tool hook: read a file
  clu-tool-write/          tool hook: create/overwrite a file
  clu-tool-ls/             tool hook: list a directory
  clu-tool-bash/           tool hook: run a bash command
  clu-input/               default input middleware (cleans user input)
  clu-output/              default output middleware (pretty-prints results)
internal/
  protocol/                ndjson wire format shared by all programs
  hookexec/                spawning hooks (direct or via --hook-shell)
  tool/                    the tool-hook contract (--describe + stdin/stdout)
```

## Roadmap

- Alternative loop hooks (multi-threaded, parallel tool execution)
- Shared-memory transport behind the same envelope format
- Streaming provider responses
- Session persistence as an input/output middleware pair

## License

[AGPL-3.0](LICENSE)
