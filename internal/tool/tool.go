// Package tool implements the tool-hook contract: `--describe` prints the
// tool's metadata as JSON; otherwise the tool reads its JSON input on stdin,
// writes its result to stdout, and exits non-zero on error.
package tool

import (
	"encoding/json"
	"fmt"
	"io"
	"os"

	"github.com/alfosua/clu/internal/protocol"
)

// Main runs a tool hook. run receives the decoded input and returns the
// result text.
func Main[In any](spec protocol.ToolSpec, run func(In) (string, error)) {
	if len(os.Args) > 1 && os.Args[1] == "--describe" {
		json.NewEncoder(os.Stdout).Encode(spec)
		return
	}
	raw, err := io.ReadAll(os.Stdin)
	if err != nil {
		fmt.Fprintln(os.Stderr, "read stdin:", err)
		os.Exit(1)
	}
	var input In
	if len(raw) > 0 {
		if err := json.Unmarshal(raw, &input); err != nil {
			fmt.Fprintln(os.Stderr, "bad input:", err)
			os.Exit(1)
		}
	}
	out, err := run(input)
	fmt.Print(out)
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
