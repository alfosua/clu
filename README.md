# clu — Core LLM Utility

An AI agent loop at the operating system level. `clu` is a distributed program
orchestration: every part of the agent loop (the loop itself, the inference
provider, each tool, and each input/output middleware) is a separate program —
a **hook** — that communicates over stdin/stdout using newline-delimited JSON.
See [DESIGN.md](DESIGN.md) for the full design.

## Layout

This is a Go monorepo: one module, one binary per `cmd/` directory, shared
code under `internal/`.

```
cmd/
  clu/                     orchestrator CLI
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
  protocol/                wire format shared by all programs
  hookexec/                spawning hooks (direct or via --hook-shell)
  tool/                    the tool-hook contract (--describe + stdin/stdout)
```

## Install

```sh
make install   # go install ./cmd/...
```

All binaries land in `$(go env GOPATH)/bin`; make sure it is on your `PATH`
so `clu` can find the default hooks.

## Usage

```sh
export ANTHROPIC_API_KEY=...
clu "Create the perfect system"
cat PROMPT.md | clu
clu -r                          # REPL mode
clu -p clu-provider-openai -m gpt-4o -u http://localhost:8080/v1/chat/completions "hi"
clu --hook-shell bash -t 'my-custom-tool --flag' "use my tool"
```

Hook names support shorthand: `-p openai` expands to `-p clu-provider-openai`,
`-t read` to `-t clu-tool-read`, etc. (prefixes: `clu-loop-`, `clu-provider-`,
`clu-tool-`, `clu-input-`, `clu-output-`). The prefixed form wins when it
exists on `PATH`; use a path (e.g. `./ls`) to force a same-named program.
Shorthand is skipped for specs containing spaces or `/`, or when
`--hook-shell` is set.

Run `clu --help` for all flags. Note two deviations from the design draft to
avoid short-flag collisions: `--version` is `-V` and `--hook-shell` has no
short form.

## Hook contracts

- **Loop**: reads `{"type":"user","text":...}` envelopes on stdin; emits
  `text`, `tool_use`, `tool_result`, `error`, and `done` envelopes on stdout.
  Configuration arrives via `CLU_*` environment variables.
- **Provider**: reads one `ProviderRequest` JSON on stdin, writes one
  `ProviderResponse` JSON on stdout.
- **Tool**: `--describe` prints its `ToolSpec` (name, description, JSON
  schema); otherwise reads its JSON input on stdin and writes its result to
  stdout, exiting non-zero on error.
- **Input/output middleware**: plain text filter — reads stdin, writes stdout.

Any executable or `--hook-shell` expression honoring a contract can replace
the built-in hooks. Shared-memory transport is planned as a future
optimization; the envelope format is transport-agnostic.
