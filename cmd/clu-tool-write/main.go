// clu-tool-write creates or overwrites a file with the given content.
package main

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"

	"github.com/alfosua/clu/internal/protocol"
	"github.com/alfosua/clu/internal/tool"
)

type input struct {
	Path    string `json:"path"`
	Content string `json:"content"`
}

func main() {
	tool.Main(protocol.ToolSpec{
		Name:        "write",
		Description: "Create or overwrite a file at the given path with the given content. Parent directories are created as needed.",
		InputSchema: json.RawMessage(`{
			"type": "object",
			"properties": {
				"path": {"type": "string", "description": "Path of the file to write"},
				"content": {"type": "string", "description": "Full content to write"}
			},
			"required": ["path", "content"]
		}`),
	}, func(in input) (string, error) {
		if dir := filepath.Dir(in.Path); dir != "." {
			if err := os.MkdirAll(dir, 0o755); err != nil {
				return "", err
			}
		}
		if err := os.WriteFile(in.Path, []byte(in.Content), 0o644); err != nil {
			return "", err
		}
		return fmt.Sprintf("wrote %d bytes to %s", len(in.Content), in.Path), nil
	})
}
