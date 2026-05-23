const std = @import("std");
const clu_data = @import("clu_data");
const DataWriter = clu_data.Writer;

pub fn main(init: std.process.Init) !void {
    var stdout_buffer: [1024 * 1024]u8 = undefined;
    var stdout_file = std.Io.File.stdout().writer(init.io, &stdout_buffer);
    const stdout = &stdout_file.interface;

    var options = try loadOptions(init);
    defer options.deinit(init.gpa);

    var custom_env = try init.minimal.environ.createMap(init.gpa);
    defer custom_env.deinit();

    var tools_json = std.Io.Writer.Allocating.init(init.gpa);
    var jws = std.json.Stringify{
        .writer = &tools_json.writer,
        .options = .{},
    };
    try jws.beginArray();
    for (options.tools.items) |tool| {
        try jws.beginWriteRaw();
        try jws.writer.writeAll(tool.schema_json);
        jws.endWriteRaw();
    }
    try jws.endArray();

    const tools_json_str = try tools_json.toOwnedSlice();
    defer init.gpa.free(tools_json_str);

    try custom_env.put("CLU_AVAILABLE_TOOLS", tools_json_str);

    const argv = &[_][]const u8{ options.provider.hook, "--base-url", "http://localhost:11434/v1", "--api-key", "ollama", "--model", "nemotron-3-nano:4b" };

    var child = try std.process.spawn(init.io, .{
        .argv = argv,
        .cwd = .inherit,
        .environ_map = &custom_env,
        .stdout = .pipe,
        .stdin = .pipe,
        .stderr = .inherit,
    });
    defer _ = child.kill(init.io);

    const child_stdin_file = child.stdin orelse return error.NoProviderStdin;
    var child_stdin_buffer: [1024 * 1024 * 4]u8 = undefined;
    var child_stdin_wrapper = child_stdin_file.writer(init.io, &child_stdin_buffer);
    const child_stdin_writer = &child_stdin_wrapper.interface;

    var data_writer = DataWriter{ .writer = child_stdin_writer };
    try data_writer.beginMessage();
    try child_stdin_writer.writeAll(options.prompt);
    try data_writer.endBlock();

    const child_stdout_file = child.stdout orelse return error.NoProviderStdout;
    var child_stdout_buffer: [1024 * 1024 * 4]u8 = undefined;
    var child_stdout_wrapper = child_stdout_file.reader(init.io, &child_stdout_buffer);
    const child_stdout_reader = &child_stdout_wrapper.interface;

    var response_id: ?[]const u8 = null;
    var tool_calls = try std.ArrayList(ToolCall).initCapacity(init.gpa, 99);
    defer tool_calls.deinit(init.gpa);
    defer for (tool_calls.items) |*tc| tc.deinit(init.gpa);

    while (try child_stdout_reader.takeDelimiter('\n')) |line| {
        if (std.mem.find(u8, line, "<response")) |start_idx| {
            const id_start_idx = std.mem.findPos(u8, line, start_idx + 9, "id=\"") orelse continue;
            const id_end_idx = std.mem.findPos(u8, line, id_start_idx + 4, "\"") orelse continue;
            const id_value = line[id_start_idx + 9 .. id_end_idx];
            response_id = id_value;
        } else if (std.mem.find(u8, line, "<function-call")) |start_idx| {
            const call_id_start_idx = std.mem.findPos(u8, line, start_idx, "call-id=\"") orelse continue;
            const call_id_end_idx = std.mem.findPos(u8, line, call_id_start_idx + 9, "\"") orelse continue;
            const call_id_value = line[call_id_start_idx + 9 .. call_id_end_idx];

            const name_start_idx = std.mem.findPos(u8, line, start_idx, "name=\"") orelse continue;
            const name_end_idx = std.mem.findPos(u8, line, name_start_idx + 6, "\"") orelse continue;
            const name_value = line[name_start_idx + 6 .. name_end_idx];

            const end_idx = std.mem.findPos(u8, line, name_end_idx + 1, ">") orelse continue;

            const params_json = line[end_idx + 1 ..];
            const tool_call = try buildToolCall(init.gpa, call_id_value, name_value, params_json);
            try tool_calls.append(init.gpa, tool_call);
        } else if (std.mem.find(u8, line, "</function-call>")) |_| {
            std.log.info("tool call", .{});
        } else if (std.mem.find(u8, line, "<reasoning")) |_| {
            try stdout.writeAll("\x1b[38;5;240m");
            try stdout.flush();
        } else if (std.mem.find(u8, line, "</reasoning>")) |_| {
            try stdout.writeAll("\n");
            try stdout.writeAll("\x1b[0m");
        } else if (std.mem.find(u8, line, "<message")) |_| {
            try stdout.writeAll("\x1b[38;5;255m");
        } else if (std.mem.find(u8, line, "</message>")) |_| {
            try stdout.writeAll("\x1b[0m");
        } else {
            try stdout.writeAll(line);
            try stdout.flush();
        }
    }

    for (tool_calls.items) |call| {
        const tool = findToolByName(options, call.name) orelse return error.ToolNotFound;

        try printToolCall(call, stdout);

        const tool_args = try buildToolProcessArgs(init.gpa, tool.*, call.params);
        defer init.gpa.free(tool_args);
        defer for (tool_args) |ta| init.gpa.free(ta);

        const result = try std.process.run(init.gpa, init.io, .{
            .argv = tool_args,
        });
        defer init.gpa.free(result.stdout);
        defer init.gpa.free(result.stderr);

        var buffer = std.Io.Writer.Allocating.init(init.gpa);
        defer buffer.deinit();

        var writer = DataWriter{ .writer = &buffer.writer };

        try writer.beginToolResult(call.call_id);
        try buffer.writer.writeAll(result.stdout);
        try buffer.writer.writeAll(result.stderr);
        try writer.endBlock();

        const result_slice = try buffer.toOwnedSlice();
        defer init.gpa.free(result_slice);

        try stdout.writeAll(result_slice);
        try stdout.flush();
    }

    const term = try child.wait(init.io);

    if (term.exited != 0) {
        std.log.err("Process failed with exit code: {d}", .{term.exited});
    }

    try stdout.writeByte('\n');
    try stdout.flush();
}

fn findToolByName(options: Options, name: []const u8) ?*ToolOptions {
    for (options.tools.items) |*tool| {
        if (std.mem.eql(u8, tool.name, name)) {
            return tool;
        }
    }
    return null;
}

fn buildToolProcessArgs(
    allocator: std.mem.Allocator,
    tool: ToolOptions,
    params: []const ToolParam,
) ![]const []const u8 {
    const initial_cap = params.len * 2 + 1;
    var tool_args = try std.ArrayList([]const u8)
        .initCapacity(allocator, initial_cap);

    try tool_args.append(allocator, try allocator.dupe(u8, tool.hook));
    for (params) |*p| {
        switch (p.*) {
            .named => |np| {
                const opt = try std.fmt.allocPrint(allocator, "--{s}", .{np.key});
                const val = try allocator.dupe(u8, np.value);
                try tool_args.append(allocator, opt);
                try tool_args.append(allocator, val);
            },
            else => return error.ToolParamNotSupported,
        }
    }

    return try tool_args.toOwnedSlice(allocator);
}

fn buildToolCall(
    allocator: std.mem.Allocator,
    call_id: []const u8,
    name: []const u8,
    params_json: []const u8,
) !ToolCall {
    const raw_json = try allocator.dupe(u8, params_json);
    errdefer allocator.free(raw_json);

    var params = try std.ArrayList(ToolParam).initCapacity(allocator, 1);
    errdefer params.deinit(allocator);

    var scanner = std.json.Scanner.initCompleteInput(allocator, raw_json);
    defer scanner.deinit();

    if (try scanner.next() != .object_begin) {
        return error.ExpectedObjectRoot;
    }

    while (true) {
        const token = try scanner.next();
        switch (token) {
            .object_end => break,
            .string => |key| {
                const value_token = try scanner.next();
                const value = switch (value_token) {
                    .string => |s| s,
                    else => return error.ExpectedStringValue,
                };
                try params.append(allocator, .{
                    .named = .{ .key = key, .value = value },
                });
            },
            else => return error.UnexpectedToken,
        }
    }

    return ToolCall{
        .call_id = call_id,
        .name = name,
        .params = try params.toOwnedSlice(allocator),
        .params_raw_json = raw_json,
    };
}

fn printToolCall(call: ToolCall, writer: *std.Io.Writer) !void {
    try writer.writeAll("\n\x1b[33m");
    try writer.writeAll("=> tool: ");
    try writer.writeAll(call.name);
    try writer.writeAll("(");
    var tailing: bool = false;
    for (call.params) |p| {
        if (tailing) try writer.writeAll(", ");
        try writer.writeAll(p.named.key);
        try writer.writeAll(": \"");
        try writer.writeAll(p.named.value);
        try writer.writeAll("\"");
        tailing = true;
    }
    try writer.writeAll(")");
    try writer.writeAll("\x1b[0m");
    try writer.writeAll("\n");
    try writer.flush();
}

const ToolCall = struct {
    call_id: []const u8,
    name: []const u8,
    params: []const ToolParam,
    params_raw_json: []const u8,

    pub fn deinit(self: *ToolCall, allocator: std.mem.Allocator) void {
        allocator.free(self.params_raw_json);
        allocator.free(self.params);
    }
};

const ToolParam = union(enum) {
    direct: []const u8,
    positional: struct {
        index: usize,
        value: []const u8,
    },
    named: struct {
        key: []const u8,
        value: []const u8,
    },
};

fn loadOptions(init: std.process.Init) !Options {
    var args = init.minimal.args.iterate();

    var prompt: ?[]const u8 = null;
    var provider_hook: ?[]const u8 = null;
    var tools: std.ArrayList(ToolOptions) = try .initCapacity(init.gpa, 2);
    while (args.next()) |arg| {
        if (std.mem.eql(u8, arg, "--provider")) {
            provider_hook = args.next() orelse {
                return error.MissingValue;
            };
        } else if (std.mem.eql(u8, arg, "--tool")) {
            const tool_hook = args.next() orelse {
                return error.MissingValue;
            };
            const tool_options = try loadToolOptions(init.gpa, init.io, tool_hook);
            try tools.append(init.gpa, tool_options);
        } else {
            prompt = arg;
        }
    }

    return Options{
        .help = false,
        .prompt = prompt orelse return error.MissingArgument,
        .tools = tools,
        .provider = .{
            .hook = provider_hook orelse return error.MissingRequiredOption,
        },
    };
}

fn loadToolOptions(
    allocator: std.mem.Allocator,
    io: std.Io,
    hook: []const u8,
) !ToolOptions {
    const result = try std.process.run(allocator, io, .{
        .argv = &[_][]const u8{ hook, "--describe" },
    });
    defer allocator.free(result.stdout);
    defer allocator.free(result.stderr);

    var schema_json_src = std.Io.Writer.Allocating.init(allocator);
    try schema_json_src.writer.writeAll(result.stdout);
    const schema_json = try schema_json_src.toOwnedSlice();

    const schema_json_parse = try std.json.parseFromSlice(std.json.Value, allocator, schema_json, .{});
    defer schema_json_parse.deinit();

    const schema_json_obj = schema_json_parse.value.object;
    const name_json_val = schema_json_obj.get("name") orelse return error.InvalidToolDefinition;
    const name = name_json_val.string;

    return ToolOptions{
        .hook = hook,
        .name = name,
        .schema_json = schema_json,
    };
}

const Options = struct {
    help: bool,
    prompt: []const u8,
    provider: ProviderOptions,
    tools: std.ArrayList(ToolOptions),

    pub fn deinit(self: *Options, allocator: std.mem.Allocator) void {
        for (self.tools.items) |*t| {
            t.deinit(allocator);
        }
        self.tools.deinit(allocator);
    }
};

const ProviderOptions = struct {
    hook: []const u8,
};

const ToolOptions = struct {
    hook: []const u8,
    name: []const u8,
    schema_json: []const u8,

    pub fn deinit(self: *ToolOptions, allocator: std.mem.Allocator) void {
        allocator.free(self.schema_json);
    }
};
