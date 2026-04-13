const std = @import("std");
const p = @import("core_providers");
const s = @import("core_storages");
const e = @import("core_extensions");

pub const Tool = p.Tool;
pub const Provider = p.Provider;
pub const Storage = s.Storage;
pub const Extension = e.Extension;
pub const Message = p.Message;

pub const RunError = error{StepLimitExceeded} || anyerror;

/// Controls a single agent run.
pub const RunOptions = struct {
    /// Hard cap on provider calls per turn. `null` means unlimited.
    max_steps: ?usize = null,
};

/// Receives text output from the agent as it is produced.
/// Wraps any concrete writer or callback behind a stable vtable.
pub const TextHandler = struct {
    ptr: *anyopaque,
    handleFn: *const fn (ptr: *anyopaque, text: []const u8) anyerror!void,

    pub fn handle(self: TextHandler, text: []const u8) !void {
        return self.handleFn(self.ptr, text);
    }
};

/// The agentic loop.
///
/// One `Agent` owns a provider, a storage backend, and a fixed set of tools
/// and extensions for the lifetime of a session. Call `run` once per user
/// turn.
pub const Agent = struct {
    allocator: std.mem.Allocator,
    provider: Provider,
    storage: Storage,
    tools: []const Tool,
    extensions: []const Extension,

    pub fn init(
        allocator: std.mem.Allocator,
        provider: Provider,
        storage: Storage,
        tools: []const Tool,
        extensions: []const Extension,
    ) Agent {
        return .{
            .allocator = allocator,
            .provider = provider,
            .storage = storage,
            .tools = tools,
            .extensions = extensions,
        };
    }

    /// Notify all registered extensions that the session has started.
    pub fn sessionStart(self: *Agent) !void {
        const handle: e.SessionHandle = self;
        for (self.extensions) |ext| try ext.onSessionStart(handle);
    }

    /// Notify all registered extensions that the session has ended.
    pub fn sessionEnd(self: *Agent) !void {
        const handle: e.SessionHandle = self;
        for (self.extensions) |ext| try ext.onSessionEnd(handle);
    }

    /// Run one agent turn.
    ///
    /// Appends `input` as a user message, then loops:
    ///   1. Fetch history and call the provider.
    ///   2. On a text response — append to history, emit via `handler`, return.
    ///   3. On tool calls — dispatch each, append results, go to 1.
    ///
    /// Returns `error.StepLimitExceeded` if `options.max_steps` is reached.
    pub fn run(
        self: *Agent,
        input: []const u8,
        options: RunOptions,
        handler: TextHandler,
    ) !void {
        try self.storage.append(.{ .user = input });

        // Gather tools from extensions alongside statically registered ones.
        var all_tools: std.ArrayList(Tool) = .{};
        defer all_tools.deinit(self.allocator);
        try all_tools.appendSlice(self.allocator, self.tools);
        for (self.extensions) |ext| try all_tools.appendSlice(self.allocator, ext.tools());

        var steps: usize = 0;
        while (true) {
            if (options.max_steps) |max| {
                if (steps >= max) return error.StepLimitExceeded;
            }
            steps += 1;

            const messages = try self.storage.history(self.allocator);
            defer self.allocator.free(messages);

            const response = try self.provider.complete(
                self.allocator,
                messages,
                all_tools.items,
            );

            switch (response) {
                .text => |text| {
                    try self.storage.append(.{ .assistant = .{ .text = text } });
                    try handler.handle(text);
                    return;
                },
                .tool_calls => |calls| {
                    try self.storage.append(.{ .assistant = .{ .tool_calls = calls } });
                    for (calls) |tc| {
                        const output = self.dispatchTool(tc, all_tools.items);
                        try self.storage.append(.{ .tool = .{
                            .call_id = tc.id,
                            .output = output,
                        } });
                    }
                },
            }
        }
    }

    /// Invoke the named tool and return its output, or a JSON error string on failure.
    fn dispatchTool(self: *Agent, tc: p.ToolCall, tools: []const Tool) p.Value {
        const tool = for (tools) |t| {
            if (std.mem.eql(u8, t.name, tc.name)) break t;
        } else {
            return .{ .string = "error: unknown tool" };
        };

        return tool.call(self.allocator, tc.input) catch |err| {
            const msg = std.fmt.allocPrint(
                self.allocator,
                "error: {s}",
                .{@errorName(err)},
            ) catch return .{ .string = "error: tool call failed" };
            return .{ .string = msg };
        };
    }
};
