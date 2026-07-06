// clu-tool-bash executes a bash command and returns its combined output.
package main

import (
	"context"
	"encoding/json"
	"os/exec"
	"time"

	"github.com/alfosua/clu/internal/protocol"
	"github.com/alfosua/clu/internal/tool"
)

type input struct {
	Command        string `json:"command"`
	TimeoutSeconds int    `json:"timeout_seconds"`
}

func main() {
	tool.Main(protocol.ToolSpec{
		Name:        "bash",
		Description: "Execute a bash command and return its combined stdout and stderr.",
		InputSchema: json.RawMessage(`{
			"type": "object",
			"properties": {
				"command": {"type": "string", "description": "The bash command to execute"},
				"timeout_seconds": {"type": "integer", "description": "Optional timeout in seconds (default 120)"}
			},
			"required": ["command"]
		}`),
	}, func(in input) (string, error) {
		timeout := 120 * time.Second
		if in.TimeoutSeconds > 0 {
			timeout = time.Duration(in.TimeoutSeconds) * time.Second
		}
		ctx, cancel := context.WithTimeout(context.Background(), timeout)
		defer cancel()
		out, err := exec.CommandContext(ctx, "bash", "-c", in.Command).CombinedOutput()
		return string(out), err
	})
}
