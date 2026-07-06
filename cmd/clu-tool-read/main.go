// clu-tool-read reads the contents of a file.
package main

import (
	"encoding/json"
	"os"

	"github.com/alfosua/clu/internal/protocol"
	"github.com/alfosua/clu/internal/tool"
)

type input struct {
	Path string `json:"path"`
}

func main() {
	tool.Main(protocol.ToolSpec{
		Name:        "read",
		Description: "Read the contents of a file at the given path.",
		InputSchema: json.RawMessage(`{
			"type": "object",
			"properties": {"path": {"type": "string", "description": "Path of the file to read"}},
			"required": ["path"]
		}`),
	}, func(in input) (string, error) {
		data, err := os.ReadFile(in.Path)
		return string(data), err
	})
}
