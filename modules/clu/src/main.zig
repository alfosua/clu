const std = @import("std");
const agent_mod = @import("core_agent");
const oai = @import("provider_openai_compat");
const mem_storage = @import("storage_memory");
const pwsh_mod = @import("tools_pwsh");
const repl_mod = @import("comms_repl");
/// Compile-time configuration baked in via `zig build -Dbase_url=... -Dapi_key=... -Dmodel=...`
const build_opts = @import("build_options");

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    const args = try std.process.argsAlloc(allocator);
    defer std.process.argsFree(allocator, args);

    // -------------------------------------------------------------------------
    // Config resolution: build-time defaults → env vars → CLI flags
    // -------------------------------------------------------------------------

    var base_url: []const u8 = build_opts.base_url;
    var api_key: []const u8 = build_opts.api_key;
    var model: []const u8 = build_opts.model;
    var system_prompt: ?[]const u8 = null;
    var quick_prompt: ?[]const u8 = null;
    var max_steps: ?usize = null;

    // Env vars override build-time defaults.
    if (std.process.getEnvVarOwned(allocator, "CLU_BASE_URL")) |v| {
        base_url = v;
    } else |_| {}
    if (std.process.getEnvVarOwned(allocator, "CLU_API_KEY")) |v| {
        api_key = v;
    } else |_| {}
    if (std.process.getEnvVarOwned(allocator, "CLU_MODEL")) |v| {
        model = v;
    } else |_| {}

    // CLI flags override env vars.
    var i: usize = 1;
    while (i < args.len) : (i += 1) {
        const arg = args[i];

        if (nextArg(args, &i, "--base-url") orelse nextArg(args, &i, "--base_url")) |v| {
            base_url = v;
        } else if (nextArg(args, &i, "--api-key") orelse nextArg(args, &i, "--api_key")) |v| {
            api_key = v;
        } else if (nextArg(args, &i, "--model")) |v| {
            model = v;
        } else if (nextArg(args, &i, "--system")) |v| {
            system_prompt = v;
        } else if (nextArg(args, &i, "--quick") orelse nextArg(args, &i, "-q")) |v| {
            quick_prompt = v;
        } else if (nextArg(args, &i, "--max-steps")) |v| {
            max_steps = std.fmt.parseInt(usize, v, 10) catch fatal("--max-steps must be a positive integer");
        } else if (std.mem.eql(u8, arg, "--help") or std.mem.eql(u8, arg, "-h")) {
            printUsage();
            return;
        } else {
            std.debug.print("unknown flag: {s}\n", .{arg});
            fatal("use --help for usage");
        }
    }

    // -------------------------------------------------------------------------
    // Wire up components
    // -------------------------------------------------------------------------

    var provider = oai.OpenAiCompatProvider.init(allocator, base_url, api_key, model);
    defer provider.deinit();

    var storage = mem_storage.MemoryStorage.init(allocator);
    defer storage.deinit();

    var pwsh = pwsh_mod.PwshTool{};
    const pwsh_tool = pwsh.tool();

    var agent = agent_mod.Agent.init(
        allocator,
        provider.provider(),
        storage.storage(),
        &.{pwsh_tool},
        &.{},
    );

    const stdout = std.fs.File.stdout();

    // -------------------------------------------------------------------------
    // Non-interactive one-shot mode (`--quick`)
    // -------------------------------------------------------------------------

    if (quick_prompt) |prompt| {
        try agent.sessionStart();
        defer agent.sessionEnd() catch {};

        if (system_prompt) |sp| {
            try agent.storage.append(.{ .system = sp });
        }

        var handler_ctx = FileHandler{ .file = stdout };
        try agent.run(prompt, .{ .max_steps = max_steps }, handler_ctx.textHandler());
        try stdout.writeAll("\n");
        return;
    }

    // -------------------------------------------------------------------------
    // Interactive REPL mode
    // -------------------------------------------------------------------------

    var repl = repl_mod.Repl.init(allocator, &agent, system_prompt);
    try repl.run();
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// If `args[i]` equals `flag`, advances `i` and returns `args[i]` (the value).
/// Returns null if the flag does not match.
fn nextArg(args: []const []const u8, i: *usize, flag: []const u8) ?[]const u8 {
    if (!std.mem.eql(u8, args[i.*], flag)) return null;
    i.* += 1;
    if (i.* >= args.len) {
        std.debug.print("flag {s} requires a value\n", .{flag});
        std.process.exit(1);
    }
    return args[i.*];
}

fn fatal(msg: []const u8) noreturn {
    std.debug.print("error: {s}\n", .{msg});
    std.process.exit(1);
}

fn printUsage() void {
    std.fs.File.stdout().writeAll(
        \\Usage: clu [options]
        \\
        \\Options:
        \\  --base-url <url>    API base URL  (env: CLU_BASE_URL)
        \\  --api-key  <key>    API key       (env: CLU_API_KEY)
        \\  --model    <id>     Model ID      (env: CLU_MODEL)
        \\  --system   <text>   System prompt for this session
        \\  --quick, -q <text>  Non-interactive: run one turn and exit
        \\  --max-steps <n>     Limit agent loop iterations
        \\  --help, -h          Show this help
        \\
        \\Compile-time defaults can be baked in at build time:
        \\  zig build -Dbase_url=http://localhost:11434 -Dapi_key=ollama -Dmodel=llama3
        \\
        \\Precedence (highest → lowest): CLI flags > env vars > build-time defaults
        \\
    ) catch {};
}

// ---------------------------------------------------------------------------
// Internal: file text handler (used in --quick mode)
// ---------------------------------------------------------------------------

const FileHandler = struct {
    file: std.fs.File,

    fn textHandler(self: *FileHandler) agent_mod.TextHandler {
        return .{
            .ptr = self,
            .handleFn = handle,
        };
    }

    fn handle(ptr: *anyopaque, text: []const u8) anyerror!void {
        const self: *FileHandler = @ptrCast(@alignCast(ptr));
        try self.file.writeAll(text);
    }
};
