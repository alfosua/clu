// clu-input is the default input middleware: it cleans basic quirks from
// user input (normalizes line endings, trims surrounding whitespace).
package main

import (
	"fmt"
	"io"
	"os"
	"strings"
)

func main() {
	data, err := io.ReadAll(os.Stdin)
	if err != nil {
		fmt.Fprintln(os.Stderr, "clu-input:", err)
		os.Exit(1)
	}
	text := strings.ReplaceAll(string(data), "\r\n", "\n")
	fmt.Print(strings.TrimSpace(text))
}
