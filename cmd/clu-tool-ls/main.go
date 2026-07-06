// clu-tool-ls lists files and directories at a given path.
package main

import (
	"encoding/json"
	"fmt"
	"os"
	"strings"

	"github.com/alfosua/clu/internal/protocol"
	"github.com/alfosua/clu/internal/tool"
)

type input struct {
	Path string `json:"path"`
}

func main() {
	tool.Main(protocol.ToolSpec{
		Name:        "ls",
		Description: "List files and directories at the given path (defaults to the current directory). Directories are suffixed with '/'.",
		InputSchema: json.RawMessage(`{
			"type": "object",
			"properties": {"path": {"type": "string", "description": "Directory to list (default \".\")"}}
		}`),
	}, func(in input) (string, error) {
		path := in.Path
		if path == "" {
			path = "."
		}
		entries, err := os.ReadDir(path)
		if err != nil {
			return "", err
		}
		var b strings.Builder
		for _, e := range entries {
			name := e.Name()
			if e.IsDir() {
				name += "/"
			}
			fmt.Fprintln(&b, name)
		}
		return b.String(), nil
	})
}
