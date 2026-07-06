// Package hookexec turns hook specifications (a program name or a shell
// expression) into runnable commands.
package hookexec

import (
	"context"
	"os"
	"os/exec"
	"strings"
)

// Command builds an *exec.Cmd for a hook spec. If shell is non-empty the
// spec is run as `shell -c <spec> [args...]`; otherwise the spec is split on
// whitespace and executed directly, with args appended.
func Command(ctx context.Context, shell, spec string, args ...string) *exec.Cmd {
	var cmd *exec.Cmd
	if shell != "" {
		cmd = exec.CommandContext(ctx, shell, append([]string{"-c", spec, shell}, args...)...)
	} else {
		fields := strings.Fields(spec)
		cmd = exec.CommandContext(ctx, fields[0], append(fields[1:], args...)...)
	}
	cmd.Env = os.Environ()
	cmd.Stderr = os.Stderr
	return cmd
}

// Run executes a hook, feeding it input on stdin and returning its stdout.
func Run(ctx context.Context, shell, spec string, input []byte, args ...string) ([]byte, error) {
	cmd := Command(ctx, shell, spec, args...)
	cmd.Stdin = strings.NewReader(string(input))
	return cmd.Output()
}
