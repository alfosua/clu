// clu-provider-openai implements the provider hook for OpenAI-compatible
// chat completions APIs. It reads a ProviderRequest JSON on stdin and writes
// a ProviderResponse JSON on stdout. Requires OPENAI_API_KEY (or a local
// endpoint via --url that ignores auth).
package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"os"
	"time"

	"github.com/alfosua/clu/internal/protocol"
)

const defaultURL = "https://api.openai.com/v1/chat/completions"
const defaultModel = "gpt-4o"

func fail(format string, args ...any) {
	json.NewEncoder(os.Stdout).Encode(protocol.ProviderResponse{Error: fmt.Sprintf(format, args...)})
	os.Exit(0)
}

type oaiMessage struct {
	Role       string        `json:"role"`
	Content    string        `json:"content,omitempty"`
	ToolCalls  []oaiToolCall `json:"tool_calls,omitempty"`
	ToolCallID string        `json:"tool_call_id,omitempty"`
}

type oaiToolCall struct {
	ID       string `json:"id"`
	Type     string `json:"type"`
	Function struct {
		Name      string `json:"name"`
		Arguments string `json:"arguments"`
	} `json:"function"`
}

func main() {
	input, err := io.ReadAll(os.Stdin)
	if err != nil {
		fail("read stdin: %v", err)
	}
	var req protocol.ProviderRequest
	if err := json.Unmarshal(input, &req); err != nil {
		fail("bad request: %v", err)
	}

	url := req.URL
	if url == "" {
		url = defaultURL
	}
	model := req.Model
	if model == "" {
		model = defaultModel
	}

	// Translate the neutral chat history into OpenAI's message format.
	var messages []oaiMessage
	if req.System != "" {
		messages = append(messages, oaiMessage{Role: "system", Content: req.System})
	}
	for _, m := range req.Messages {
		switch m.Role {
		case "assistant":
			msg := oaiMessage{Role: "assistant"}
			for _, b := range m.Content {
				switch b.Type {
				case "text":
					msg.Content += b.Text
				case "tool_use":
					tc := oaiToolCall{ID: b.ID, Type: "function"}
					tc.Function.Name = b.Name
					tc.Function.Arguments = string(b.Input)
					msg.ToolCalls = append(msg.ToolCalls, tc)
				}
			}
			messages = append(messages, msg)
		default: // user turns may carry text and/or tool results
			for _, b := range m.Content {
				switch b.Type {
				case "text":
					messages = append(messages, oaiMessage{Role: "user", Content: b.Text})
				case "tool_result":
					messages = append(messages, oaiMessage{Role: "tool", Content: b.Content, ToolCallID: b.ToolUseID})
				}
			}
		}
	}

	body := map[string]any{"model": model, "messages": messages}
	if len(req.Tools) > 0 {
		var tools []map[string]any
		for _, t := range req.Tools {
			tools = append(tools, map[string]any{
				"type": "function",
				"function": map[string]any{
					"name":        t.Name,
					"description": t.Description,
					"parameters":  t.InputSchema,
				},
			})
		}
		body["tools"] = tools
	}
	if req.Effort != "" {
		body["reasoning_effort"] = req.Effort
	}
	payload, _ := json.Marshal(body)

	httpReq, err := http.NewRequest("POST", url, bytes.NewReader(payload))
	if err != nil {
		fail("build request: %v", err)
	}
	httpReq.Header.Set("content-type", "application/json")
	if key := os.Getenv("OPENAI_API_KEY"); key != "" {
		httpReq.Header.Set("authorization", "Bearer "+key)
	}

	client := &http.Client{Timeout: 5 * time.Minute}
	resp, err := client.Do(httpReq)
	if err != nil {
		fail("request failed: %v", err)
	}
	defer resp.Body.Close()
	raw, _ := io.ReadAll(resp.Body)
	if resp.StatusCode != http.StatusOK {
		fail("api error (%d): %s", resp.StatusCode, raw)
	}

	var apiResp struct {
		Choices []struct {
			Message      oaiMessage `json:"message"`
			FinishReason string     `json:"finish_reason"`
		} `json:"choices"`
	}
	if err := json.Unmarshal(raw, &apiResp); err != nil || len(apiResp.Choices) == 0 {
		fail("bad api response: %s", raw)
	}

	choice := apiResp.Choices[0]
	var content []protocol.ContentBlock
	if choice.Message.Content != "" {
		content = append(content, protocol.ContentBlock{Type: "text", Text: choice.Message.Content})
	}
	stop := "end_turn"
	for _, tc := range choice.Message.ToolCalls {
		content = append(content, protocol.ContentBlock{
			Type:  "tool_use",
			ID:    tc.ID,
			Name:  tc.Function.Name,
			Input: json.RawMessage(tc.Function.Arguments),
		})
		stop = "tool_use"
	}
	json.NewEncoder(os.Stdout).Encode(protocol.ProviderResponse{
		Message:    protocol.ChatMessage{Role: "assistant", Content: content},
		StopReason: stop,
	})
}
