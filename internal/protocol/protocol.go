// Package protocol defines the wire format used between clu programs.
//
// All hooks communicate over stdin/stdout using newline-delimited JSON
// envelopes. Shared-memory transport is a planned optimization; the
// envelope format is transport-agnostic to allow that later.
package protocol

import (
	"bufio"
	"encoding/json"
	"io"
)

// Envelope types exchanged between the orchestrator, loop, and hooks.
const (
	TypeUser       = "user"        // user prompt -> loop
	TypeText       = "text"        // assistant text output
	TypeToolUse    = "tool_use"    // loop announces a tool invocation
	TypeToolResult = "tool_result" // loop announces a tool result
	TypeError      = "error"
	TypeDone       = "done" // loop finished processing one user turn
)

// Envelope is a single protocol message.
type Envelope struct {
	Type string `json:"type"`
	Text string `json:"text,omitempty"`
	Name string `json:"name,omitempty"` // tool name for tool_use/tool_result
	Data any    `json:"data,omitempty"`
}

// ToolSpec is the metadata a tool hook emits when invoked with --describe.
type ToolSpec struct {
	Name        string          `json:"name"`
	Description string          `json:"description"`
	InputSchema json.RawMessage `json:"input_schema"`
}

// ContentBlock is one piece of a chat message (text, tool call, or tool result).
type ContentBlock struct {
	Type      string          `json:"type"` // "text" | "tool_use" | "tool_result"
	Text      string          `json:"text,omitempty"`
	ID        string          `json:"id,omitempty"`
	Name      string          `json:"name,omitempty"`
	Input     json.RawMessage `json:"input,omitempty"`
	ToolUseID string          `json:"tool_use_id,omitempty"`
	Content   string          `json:"content,omitempty"`
	IsError   bool            `json:"is_error,omitempty"`
}

// ChatMessage is one turn in the conversation passed to a provider.
type ChatMessage struct {
	Role    string         `json:"role"` // "user" | "assistant"
	Content []ContentBlock `json:"content"`
}

// ProviderRequest is what the loop writes to a provider hook's stdin.
type ProviderRequest struct {
	Model    string        `json:"model,omitempty"`
	URL      string        `json:"url,omitempty"`
	Effort   string        `json:"effort,omitempty"`
	System   string        `json:"system,omitempty"`
	Messages []ChatMessage `json:"messages"`
	Tools    []ToolSpec    `json:"tools,omitempty"`
}

// ProviderResponse is what a provider hook writes to stdout.
type ProviderResponse struct {
	Message    ChatMessage `json:"message"`
	StopReason string      `json:"stop_reason"` // "end_turn" | "tool_use" | ...
	Error      string      `json:"error,omitempty"`
}

// Writer emits newline-delimited JSON values.
type Writer struct{ enc *json.Encoder }

func NewWriter(w io.Writer) *Writer { return &Writer{enc: json.NewEncoder(w)} }

func (w *Writer) Write(v any) error { return w.enc.Encode(v) }

// Reader reads newline-delimited JSON values.
type Reader struct{ sc *bufio.Scanner }

func NewReader(r io.Reader) *Reader {
	sc := bufio.NewScanner(r)
	sc.Buffer(make([]byte, 0, 64*1024), 16*1024*1024)
	return &Reader{sc: sc}
}

// Read decodes the next line into v. Returns io.EOF when the stream ends.
func (r *Reader) Read(v any) error {
	for r.sc.Scan() {
		line := r.sc.Bytes()
		if len(line) == 0 {
			continue
		}
		return json.Unmarshal(line, v)
	}
	if err := r.sc.Err(); err != nil {
		return err
	}
	return io.EOF
}
