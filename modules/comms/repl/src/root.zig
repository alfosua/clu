const std = @import("std");
const builtin = @import("builtin");
const a = @import("core_agent");

/// Interactive REPL session.
///
/// Wraps an `Agent` with a terminal prompt, built-in commands,
/// shell-prefix shortcuts, and multi-line input.
pub const Repl = struct {
    allocator: std.mem.Allocator,
    agent: *a.Agent,
    system_prompt: ?[]const u8,

    pub fn init(
        allocator: std.mem.Allocator,
        agent: *a.Agent,
        system_prompt: ?[]const u8,
    ) Repl {
        return .{
            .allocator = allocator,
            .agent = agent,
            .system_prompt = system_prompt,
        };
    }

    pub fn run(self: *Repl) !void {
        const stdin = std.fs.File.stdin();
        const stdout = std.fs.File.stdout();

        try self.agent.sessionStart();
        defer self.agent.sessionEnd() catch {};

        if (self.system_prompt) |sp| {
            try self.agent.storage.append(.{ .system = sp });
        }

        try stdout.writeAll("Welcome to clu. Type /help for commands, Ctrl+D to exit.\n");

        var out_handler = OutHandler{ .file = stdout };
        const text_handler = out_handler.textHandler();

        var line_buf: std.ArrayList(u8) = .{};
        defer line_buf.deinit(self.allocator);

        while (true) {
            line_buf.clearRetainingCapacity();
            try stdout.writeAll(">>> ");

            // Read first line; EOF (Ctrl+D) exits cleanly.
            stdin.reader().streamUntilDelimiter(line_buf.writer(), '\n', null) catch |err| switch (err) {
                error.EndOfStream => {
                    try stdout.writeAll("\n");
                    return;
                },
                else => return err,
            };

            // Multi-line continuation: trailing `\` on the line.
            while (std.mem.endsWith(u8, line_buf.items, "\\")) {
                line_buf.items.len -= 1; // drop the backslash
                try stdout.writeAll("... ");
                stdin.reader().streamUntilDelimiter(line_buf.writer(), '\n', null) catch |err| switch (err) {
                    error.EndOfStream => break,
                    else => return err,
                };
            }

            const input = std.mem.trim(u8, line_buf.items, " \t\r");
            if (input.len == 0) continue;

            // Built-in commands (/reset, /history, …)
            if (input[0] == '/') {
                self.handleCommand(input) catch |err| {
                    try stdout.writer().print("error: {s}\n", .{@errorName(err)});
                };
                continue;
            }

            // `!!cmd` — run shell, do NOT inject output into context.
            if (std.mem.startsWith(u8, input, "!!")) {
                const cmd = std.mem.trim(u8, input[2..], " \t");
                self.runShell(cmd, false) catch |err| {
                    try stdout.writer().print("error: {s}\n", .{@errorName(err)});
                };
                continue;
            }

            // `!cmd` — run shell and inject output into context.
            if (input[0] == '!') {
                const cmd = std.mem.trim(u8, input[1..], " \t");
                self.runShell(cmd, true) catch |err| {
                    try stdout.writer().print("error: {s}\n", .{@errorName(err)});
                };
                continue;
            }

            // Regular agent turn.
            try stdout.writeAll("\n");
            self.agent.run(input, .{}, text_handler) catch |err| {
                try stdout.writer().print("error: {s}\n", .{@errorName(err)});
            };
            try stdout.writeAll("\n");
        }
    }

    fn handleCommand(self: *Repl, raw: []const u8) !void {
        const stdout = std.fs.File.stdout();
        const w = stdout.writer();

        // Strip leading `/`; split off the command word.
        const rest = std.mem.trim(u8, raw[1..], " \t");
        const word_end = std.mem.indexOfScalar(u8, rest, ' ');
        const cmd = if (word_end) |n| rest[0..n] else rest;

        if (std.mem.eql(u8, cmd, "quit") or std.mem.eql(u8, cmd, "exit")) {
            try stdout.writeAll("\n");
            std.process.exit(0);
        } else if (std.mem.eql(u8, cmd, "reset")) {
            try self.agent.storage.clear();
            if (self.system_prompt) |sp| {
                try self.agent.storage.append(.{ .system = sp });
            }
            try w.writeAll("Session reset.\n");
        } else if (std.mem.eql(u8, cmd, "history")) {
            const msgs = try self.agent.storage.history(self.allocator);
            defer self.allocator.free(msgs);
            for (msgs, 0..) |msg, i| {
                const role = switch (msg) {
                    .system => "system",
                    .user => "user",
                    .assistant => "assistant",
                    .tool => "tool",
                };
                try w.print("[{d}] {s}\n", .{ i, role });
            }
        } else if (std.mem.eql(u8, cmd, "tools")) {
            for (self.agent.tools) |tool| {
                try w.print("  {s} — {s}\n", .{ tool.name, tool.description });
            }
        } else if (std.mem.eql(u8, cmd, "help")) {
            try w.writeAll(
                \\/reset    — clear session history
                \\/history  — print message history (roles only)
                \\/tools    — list registered tools
                \\/help     — show this message
                \\/quit     — exit  (also /exit or Ctrl+D)
                \\
                \\  !<cmd>  — run shell command, add output to context
                \\ !!<cmd>  — run shell command, do not add to context
                \\
            );
        } else {
            try w.print("Unknown command: /{s}  (try /help)\n", .{cmd});
        }
    }

    fn runShell(self: *Repl, cmd: []const u8, inject_context: bool) !void {
        const stdout = std.fs.File.stdout();
        const w = stdout.writer();

        // Build argv for the platform shell.
        var argv: std.ArrayList([]const u8) = .{};
        defer argv.deinit(self.allocator);
        if (builtin.os.tag == .windows) {
            try argv.appendSlice(self.allocator, &.{ "pwsh", "-Command", cmd });
        } else {
            try argv.appendSlice(self.allocator, &.{ "sh", "-c", cmd });
        }

        const result = try std.process.Child.run(.{
            .allocator = self.allocator,
            .argv = argv.items,
            .max_output_bytes = 4 * 1024 * 1024,
        });
        defer self.allocator.free(result.stdout);
        defer self.allocator.free(result.stderr);

        try w.writeAll("\n");
        if (result.stdout.len > 0) try w.writeAll(result.stdout);
        if (result.stderr.len > 0) try w.writeAll(result.stderr);
        try w.writeAll("\n");

        if (inject_context) {
            // Allocate message without freeing — storage borrows the slice for
            // the lifetime of the session (which equals the process lifetime).
            const msg = try std.fmt.allocPrint(
                self.allocator,
                "$ {s}\n{s}{s}",
                .{ cmd, result.stdout, result.stderr },
            );
            try self.agent.storage.append(.{ .user = msg });
            try w.writeAll("(output added to context)\n");
        } else {
            try w.writeAll("(not added to context)\n");
        }
    }
};

// ---------------------------------------------------------------------------
// Internal: stdout text handler
// ---------------------------------------------------------------------------

const OutHandler = struct {
    file: std.fs.File,

    fn textHandler(self: *OutHandler) a.TextHandler {
        return .{
            .ptr = self,
            .handleFn = handle,
        };
    }

    fn handle(ptr: *anyopaque, text: []const u8) anyerror!void {
        const self: *OutHandler = @ptrCast(@alignCast(ptr));
        try self.file.writeAll(text);
    }
};
