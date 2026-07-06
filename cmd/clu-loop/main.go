// clu-loop is the default single-threaded agent loop hook.
//
// It reads user envelopes from stdin, drives the provider hook until the
// model stops asking for tools, and emits text/tool/done envelopes on stdout.
// Hook configuration arrives via CLU_* environment variables set by clu.
package main

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"strings"

	"github.com/alfosua/clu/internal/hookexec"
	"github.com/alfosua/clu/internal/protocol"
)

func main() {
	ctx := context.Background()
	shell := os.Getenv("CLU_HOOK_SHELL")
	provider := os.Getenv("CLU_PROVIDER")
	if provider == "" {
		provider = "clu-provider-anthropic"
	}

	var toolSpecs []protocol.ToolSpec
	toolHooks := map[string]string{} // tool name -> hook spec
	for _, spec := range strings.Split(os.Getenv("CLU_TOOLS"), "\x1f") {
		if spec == "" {
			continue
		}
		out, err := hookexec.Run(ctx, shell, spec, nil, "--describe")
		if err != nil {
			fmt.Fprintf(os.Stderr, "clu-loop: cannot describe tool %q: %v\n", spec, err)
			continue
		}
		var ts protocol.ToolSpec
		if err := json.Unmarshal(out, &ts); err != nil {
			fmt.Fprintf(os.Stderr, "clu-loop: bad describe output from %q: %v\n", spec, err)
			continue
		}
		toolSpecs = append(toolSpecs, ts)
		toolHooks[ts.Name] = spec
	}

	in := protocol.NewReader(os.Stdin)
	out := protocol.NewWriter(os.Stdout)
	var history []protocol.ChatMessage

	for {
		var env protocol.Envelope
		if err := in.Read(&env); err != nil {
			if err != io.EOF {
				fmt.Fprintln(os.Stderr, "clu-loop:", err)
				os.Exit(1)
			}
			return
		}
		if env.Type != protocol.TypeUser {
			continue
		}
		history = append(history, protocol.ChatMessage{
			Role:    "user",
			Content: []protocol.ContentBlock{{Type: "text", Text: env.Text}},
		})

		for {
			req, _ := json.Marshal(protocol.ProviderRequest{
				Model:    os.Getenv("CLU_MODEL"),
				URL:      os.Getenv("CLU_URL"),
				Effort:   os.Getenv("CLU_EFFORT"),
				System:   os.Getenv("CLU_SYSTEM_PROMPT"),
				Messages: history,
				Tools:    toolSpecs,
			})
			raw, err := hookexec.Run(ctx, shell, provider, req)
			if err != nil {
				out.Write(protocol.Envelope{Type: protocol.TypeError, Text: fmt.Sprintf("provider %q failed: %v", provider, err)})
				break
			}
			var resp protocol.ProviderResponse
			if err := json.Unmarshal(raw, &resp); err != nil {
				out.Write(protocol.Envelope{Type: protocol.TypeError, Text: "bad provider response: " + err.Error()})
				break
			}
			if resp.Error != "" {
				out.Write(protocol.Envelope{Type: protocol.TypeError, Text: resp.Error})
				break
			}
			history = append(history, resp.Message)

			var results []protocol.ContentBlock
			for _, block := range resp.Message.Content {
				switch block.Type {
				case "text":
					out.Write(protocol.Envelope{Type: protocol.TypeText, Text: block.Text})
				case "tool_use":
					out.Write(protocol.Envelope{Type: protocol.TypeToolUse, Name: block.Name, Data: block.Input})
					result := runTool(ctx, shell, toolHooks, block)
					out.Write(protocol.Envelope{Type: protocol.TypeToolResult, Name: block.Name})
					results = append(results, result)
				}
			}
			if len(results) == 0 {
				break // end of turn
			}
			history = append(history, protocol.ChatMessage{Role: "user", Content: results})
		}
		out.Write(protocol.Envelope{Type: protocol.TypeDone})
	}
}

func runTool(ctx context.Context, shell string, hooks map[string]string, call protocol.ContentBlock) protocol.ContentBlock {
	result := protocol.ContentBlock{Type: "tool_result", ToolUseID: call.ID}
	spec, ok := hooks[call.Name]
	if !ok {
		result.Content = "unknown tool: " + call.Name
		result.IsError = true
		return result
	}
	out, err := hookexec.Run(ctx, shell, spec, call.Input)
	result.Content = string(out)
	if err != nil {
		result.IsError = true
		if result.Content == "" {
			result.Content = err.Error()
		}
	}
	return result
}
