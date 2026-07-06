// clu-output is the default output middleware: it pretty-prints results on
// stdout, ensuring output ends with exactly one trailing newline.
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
		fmt.Fprintln(os.Stderr, "clu-output:", err)
		os.Exit(1)
	}
	fmt.Println(strings.TrimRight(string(data), "\n"))
}
