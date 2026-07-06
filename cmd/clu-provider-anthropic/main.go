// clu-provider-anthropic implements the provider hook for Anthropic's
// Messages API. It reads a ProviderRequest JSON on stdin and writes a
// ProviderResponse JSON on stdout. Requires ANTHROPIC_API_KEY.
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

const defaultURL = "https://api.anthropic.com/v1/messages"
const defaultModel = "claude-sonnet-5"

func fail(format string, args ...any) {
	json.NewEncoder(os.Stdout).Encode(protocol.ProviderResponse{Error: fmt.Sprintf(format, args...)})
	os.Exit(0)
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

	key := os.Getenv("ANTHROPIC_API_KEY")
	if key == "" {
		fail("ANTHROPIC_API_KEY is not set")
	}
	url := req.URL
	if url == "" {
		url = defaultURL
	}
	model := req.Model
	if model == "" {
		model = defaultModel
	}

	body := map[string]any{
		"model":      model,
		"max_tokens": 8192,
		"messages":   req.Messages,
	}
	if req.System != "" {
		body["system"] = req.System
	}
	if len(req.Tools) > 0 {
		body["tools"] = req.Tools
	}
	if req.Effort != "" {
		body["thinking"] = map[string]any{"type": "enabled", "budget_tokens": effortBudget(req.Effort)}
	}
	payload, _ := json.Marshal(body)

	httpReq, err := http.NewRequest("POST", url, bytes.NewReader(payload))
	if err != nil {
		fail("build request: %v", err)
	}
	httpReq.Header.Set("content-type", "application/json")
	httpReq.Header.Set("x-api-key", key)
	httpReq.Header.Set("anthropic-version", "2023-06-01")

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
		Content    []protocol.ContentBlock `json:"content"`
		StopReason string                  `json:"stop_reason"`
	}
	if err := json.Unmarshal(raw, &apiResp); err != nil {
		fail("bad api response: %v", err)
	}

	// Drop thinking blocks; keep text and tool_use.
	var content []protocol.ContentBlock
	for _, b := range apiResp.Content {
		if b.Type == "text" || b.Type == "tool_use" {
			content = append(content, b)
		}
	}
	json.NewEncoder(os.Stdout).Encode(protocol.ProviderResponse{
		Message:    protocol.ChatMessage{Role: "assistant", Content: content},
		StopReason: apiResp.StopReason,
	})
}

func effortBudget(effort string) int {
	switch effort {
	case "low":
		return 1024
	case "medium":
		return 4096
	case "high":
		return 16384
	default:
		return 4096
	}
}
