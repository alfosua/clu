// clu is the orchestrator: it collects the prompt, runs input middlewares,
// spawns the loop hook, and renders loop output through output middlewares.
package main

import (
	"bufio"
	"context"
	"fmt"
	"io"
	"os"
	"os/exec"
	"strings"

	"github.com/alfosua/clu/internal/hookexec"
	"github.com/alfosua/clu/internal/protocol"
)

const version = "0.1.0"

const usage = `Core LLM Utility v` + version + `
An AI agent loop at the operating system level

USAGE:
    clu [FLAGS] [OPTIONS] [PROMPT]

FLAGS:
    -h, --help     Prints help information
    -V, --version  Prints version information
    -r, --repl     Enters REPL mode after first prompt

OPTIONS:
        --hook-shell <SHELL>          Specifies hook shell, none runs direct process
    -l, --loop <LOOP>                 Specifies loop implementation hook
    -p, --provider <PROVIDER>         Specifies inference provider hook
    -u, --url <URL>                   Specifies target endpoint URL used by the provider if necessary
    -m, --model <MODEL_ID>            Specifies model by ID to be chosen through the provider
    -e, --effort <LEVEL>              Specifies effort level for the model (if not specified, use default by provider)
    -s, --system-prompt <PROMPT>      Overwrites the system prompt
    -a, --add-system-prompt <PROMPT>  Adds to the system prompt
    -t, --tool <TOOL>                 Adds a tool hook and makes it available for the agent loop
    -i, --input <INPUT>               Adds a middleware hook to the input pipeline (order matters)
    -o, --output <OUTPUT>             Adds a middleware hook to the output pipeline (order matters)
    -v, --verbose <LEVEL>             Set logging verbosity: 0 (none), 1 (error), 2 (info), 3 (debug)

ARGUMENTS:
    [PROMPT]  A fragment of the prompt message

EXAMPLES:
    clu "Create the perfect system"
    cat PROMPT.md | clu
`

type config struct {
	repl         bool
	hookShell    string
	loop         string
	provider     string
	url          string
	model        string
	effort       string
	systemPrompt string
	addSystem    []string
	tools        []string
	inputs       []string
	outputs      []string
	verbose      string
	prompt       []string
}

func parseArgs(args []string) (*config, error) {
	c := &config{loop: "clu-loop", provider: "clu-provider-anthropic", verbose: "1"}
	needValue := func(i *int, name string) (string, error) {
		*i++
		if *i >= len(args) {
			return "", fmt.Errorf("option %s requires a value", name)
		}
		return args[*i], nil
	}
	for i := 0; i < len(args); i++ {
		a := args[i]
		var err error
		switch a {
		case "-h", "--help":
			fmt.Print(usage)
			os.Exit(0)
		case "-V", "--version":
			fmt.Println("clu " + version)
			os.Exit(0)
		case "-r", "--repl":
			c.repl = true
		case "--hook-shell":
			c.hookShell, err = needValue(&i, a)
		case "-l", "--loop":
			c.loop, err = needValue(&i, a)
		case "-p", "--provider":
			c.provider, err = needValue(&i, a)
		case "-u", "--url":
			c.url, err = needValue(&i, a)
		case "-m", "--model":
			c.model, err = needValue(&i, a)
		case "-e", "--effort":
			c.effort, err = needValue(&i, a)
		case "-s", "--system-prompt":
			c.systemPrompt, err = needValue(&i, a)
		case "-a", "--add-system-prompt":
			var v string
			v, err = needValue(&i, a)
			c.addSystem = append(c.addSystem, v)
		case "-t", "--tool":
			var v string
			v, err = needValue(&i, a)
			c.tools = append(c.tools, v)
		case "-i", "--input":
			var v string
			v, err = needValue(&i, a)
			c.inputs = append(c.inputs, v)
		case "-o", "--output":
			var v string
			v, err = needValue(&i, a)
			c.outputs = append(c.outputs, v)
		case "-v", "--verbose":
			c.verbose, err = needValue(&i, a)
		default:
			if strings.HasPrefix(a, "-") && a != "-" {
				return nil, fmt.Errorf("unknown flag: %s", a)
			}
			c.prompt = append(c.prompt, a)
		}
		if err != nil {
			return nil, err
		}
	}
	if len(c.tools) == 0 {
		c.tools = []string{"clu-tool-read", "clu-tool-write", "clu-tool-ls", "clu-tool-bash"}
	}
	if len(c.inputs) == 0 {
		c.inputs = []string{"clu-input"}
	}
	if len(c.outputs) == 0 {
		c.outputs = []string{"clu-output"}
	}
	c.resolveSugar()
	return c, nil
}

// resolveSugar expands shorthand hook names: `-p openai` means
// `-p clu-provider-openai` whenever that binary exists on PATH; the prefixed
// form takes priority over a same-named program (use a path like ./ls to
// force one). Specs with a path separator, spaces, or a custom --hook-shell
// are left untouched.
func (c *config) resolveSugar() {
	sugar := func(prefix, spec string) string {
		if c.hookShell != "" || strings.ContainsAny(spec, " /") {
			return spec
		}
		if _, err := exec.LookPath(prefix + spec); err == nil {
			return prefix + spec
		}
		return spec
	}
	sugarAll := func(prefix string, specs []string) {
		for i, s := range specs {
			specs[i] = sugar(prefix, s)
		}
	}
	c.loop = sugar("clu-loop-", c.loop)
	c.provider = sugar("clu-provider-", c.provider)
	sugarAll("clu-tool-", c.tools)
	sugarAll("clu-input-", c.inputs)
	sugarAll("clu-output-", c.outputs)
}

func (c *config) applyMiddlewares(ctx context.Context, specs []string, text string) string {
	for _, spec := range specs {
		out, err := hookexec.Run(ctx, c.hookShell, spec, []byte(text))
		if err != nil {
			fmt.Fprintf(os.Stderr, "clu: middleware %q failed: %v\n", spec, err)
			continue
		}
		text = string(out)
	}
	return text
}

func main() {
	cfg, err := parseArgs(os.Args[1:])
	if err != nil {
		fmt.Fprintln(os.Stderr, "clu:", err)
		os.Exit(2)
	}
	ctx := context.Background()

	system := cfg.systemPrompt
	if system == "" {
		system = "You are clu, a helpful AI agent running at the operating system level. Use the available tools to accomplish the user's task."
	}
	if len(cfg.addSystem) > 0 {
		system += "\n\n" + strings.Join(cfg.addSystem, "\n\n")
	}

	// Assemble the first prompt from args and piped stdin.
	prompt := strings.Join(cfg.prompt, " ")
	stdinIsPipe := false
	if fi, err := os.Stdin.Stat(); err == nil && fi.Mode()&os.ModeCharDevice == 0 {
		stdinIsPipe = true
		piped, _ := io.ReadAll(os.Stdin)
		if len(piped) > 0 {
			if prompt != "" {
				prompt += "\n\n"
			}
			prompt += string(piped)
		}
	}
	if strings.TrimSpace(prompt) == "" && !cfg.repl {
		fmt.Print(usage)
		os.Exit(2)
	}

	// Spawn the loop hook; hook configuration travels via environment.
	loopCmd := hookexec.Command(ctx, cfg.hookShell, cfg.loop)
	loopCmd.Env = append(loopCmd.Env,
		"CLU_PROVIDER="+cfg.provider,
		"CLU_TOOLS="+strings.Join(cfg.tools, "\x1f"),
		"CLU_HOOK_SHELL="+cfg.hookShell,
		"CLU_MODEL="+cfg.model,
		"CLU_URL="+cfg.url,
		"CLU_EFFORT="+cfg.effort,
		"CLU_SYSTEM_PROMPT="+system,
		"CLU_VERBOSE="+cfg.verbose,
	)
	loopIn, err := loopCmd.StdinPipe()
	if err != nil {
		fmt.Fprintln(os.Stderr, "clu:", err)
		os.Exit(1)
	}
	loopOut, err := loopCmd.StdoutPipe()
	if err != nil {
		fmt.Fprintln(os.Stderr, "clu:", err)
		os.Exit(1)
	}
	if err := loopCmd.Start(); err != nil {
		fmt.Fprintf(os.Stderr, "clu: cannot start loop hook %q: %v\n", cfg.loop, err)
		os.Exit(1)
	}

	toLoop := protocol.NewWriter(loopIn)
	fromLoop := protocol.NewReader(loopOut)

	sendTurn := func(text string) bool {
		text = cfg.applyMiddlewares(ctx, cfg.inputs, text)
		if err := toLoop.Write(protocol.Envelope{Type: protocol.TypeUser, Text: text}); err != nil {
			fmt.Fprintln(os.Stderr, "clu:", err)
			return false
		}
		for {
			var env protocol.Envelope
			if err := fromLoop.Read(&env); err != nil {
				if err != io.EOF {
					fmt.Fprintln(os.Stderr, "clu:", err)
				}
				return false
			}
			switch env.Type {
			case protocol.TypeText:
				fmt.Print(cfg.applyMiddlewares(ctx, cfg.outputs, env.Text))
			case protocol.TypeToolUse:
				fmt.Fprintf(os.Stderr, "[tool: %s]\n", env.Name)
			case protocol.TypeError:
				fmt.Fprintln(os.Stderr, "clu: loop error:", env.Text)
			case protocol.TypeDone:
				return true
			}
		}
	}

	ok := true
	if strings.TrimSpace(prompt) != "" {
		ok = sendTurn(prompt)
	}

	if cfg.repl && ok && !stdinIsPipe {
		sc := bufio.NewScanner(os.Stdin)
		for {
			fmt.Fprint(os.Stderr, "clu> ")
			if !sc.Scan() {
				break
			}
			line := strings.TrimSpace(sc.Text())
			if line == "" {
				continue
			}
			if line == "exit" || line == "quit" {
				break
			}
			if !sendTurn(line) {
				break
			}
		}
	}

	loopIn.Close()
	if err := loopCmd.Wait(); err != nil && ok {
		fmt.Fprintln(os.Stderr, "clu: loop exited:", err)
		os.Exit(1)
	}
	if !ok {
		os.Exit(1)
	}
}
