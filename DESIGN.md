# Design document

## Help proposal

```
Core LLM Utility v0.1
An AI agent loop at the operating system level

USAGE:
    clu [FLAGS] [OPTIONS] [PROMPT]

FLAGS:
    -h, --help     Prints help information
    -v, --version  Prints version information
    -r, --repl     Enters REPL mode after first prompt

OPTIONS:
    -h, --hook-shell <SHELL>          Specifies hook shell, none runs direct process
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
```

## Hooks

A hook is a program or shell expression that is used to process a particular task in the loop.

1. Loop: hook that implements the main orchestration logic for the agent loop
2. Provider: hook that implements the inference calls
3. Tools: hooks that serve as tools for the agent loop to use with the provider
4. Input middlewares: hooks that runs when an input of any kind is received
5. Output middlewares: hooks that runs when an output of any kind is received

### Loops

Mainly, there would be a few implementations, like single-threaded, multi-threaded, with experimental tecnniques and so on.

### Providers

Mainly, there would be API call implementations for HTTP clients to the OpenAI-compatible APIs like responses and chat, and also Anthropic's message API. Later on, there will be implementation for bare metal usage to LLM protocols.

### Tools

They are average programs that are designed to be used as a tool during agent loops. Offering special options to quickly offer metadata about the tool for the LLM.

Between the ones that we want to implement:

- read: read the contents of a file
- write: create or modify contents of a file
- ls: list files and directories given a path
- bash: executes bash commands

### Input & Output Middlewares

They are simple middlewares that could handle input or output respectively for multiple use cases. They could be use for session administration, prompt injection, extensions, enhancing input/output, etc.

Normally there would be the regular input and output that cleans basic quirks from user input and pretty-prints the results on stdout.

### Shell

You could define a particular shell for more advance hook definitions.

## Communication Protocol

As this is a distributed program orchestration, they need to communicate passing binary data. They could leverage OS shared memory techniques to reduce IO pipelines exhaustion.
