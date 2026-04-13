const std = @import("std");
const p = @import("core_providers");

/// OpenAI-compatible Chat Completions provider.
///
/// Works with OpenAI, Ollama, and any API that speaks the
/// `/v1/chat/completions` contract.
pub const OpenAiCompatProvider = struct {
    allocator: std.mem.Allocator,
    base_url: []const u8,
    api_key: []const u8,
    model: []const u8,
    client: std.http.Client,

    pub fn init(
        allocator: std.mem.Allocator,
        base_url: []const u8,
        api_key: []const u8,
        model: []const u8,
    ) OpenAiCompatProvider {
        return .{
            .allocator = allocator,
            .base_url = base_url,
            .api_key = api_key,
            .model = model,
            .client = std.http.Client{ .allocator = allocator },
        };
    }

    pub fn deinit(self: *OpenAiCompatProvider) void {
        self.client.deinit();
    }

    /// Return a `Provider` vtable backed by this instance.
    /// The caller must keep `self` alive for the lifetime of the returned `Provider`.
    pub fn provider(self: *OpenAiCompatProvider) p.Provider {
        return .{
            .ptr = self,
            .completeFn = completeFn,
        };
    }

    fn completeFn(
        ptr: *anyopaque,
        allocator: std.mem.Allocator,
        messages: []const p.Message,
        tools: []const p.Tool,
    ) anyerror!p.Response {
        const self: *OpenAiCompatProvider = @ptrCast(@alignCast(ptr));

        const url = try std.fmt.allocPrint(
            allocator,
            "{s}/v1/chat/completions",
            .{self.base_url},
        );
        defer allocator.free(url);

        const auth = try std.fmt.allocPrint(allocator, "Bearer {s}", .{self.api_key});
        defer allocator.free(auth);

        const body = try buildRequestBody(allocator, self.model, messages, tools);
        defer allocator.free(body);

        var response_body: std.ArrayList(u8) = .{};
        defer response_body.deinit(allocator);

        const result = try self.client.fetch(.{
            .method = .POST,
            .location = .{ .url = url },
            .extra_headers = &.{
                .{ .name = "Authorization", .value = auth },
                .{ .name = "Content-Type", .value = "application/json" },
            },
            .payload = body,
            .response_storage = .{ .dynamic = &response_body },
        });

        if (result.status.class() != .success) {
            return error.ProviderHttpError;
        }

        return parseResponse(allocator, response_body.items);
    }
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn jsonStringifyAlloc(allocator: std.mem.Allocator, value: anytype) ![]u8 {
    return std.fmt.allocPrint(allocator, "{f}", .{std.json.fmt(value, .{})});
}

// ---------------------------------------------------------------------------
// Request serialization
// ---------------------------------------------------------------------------

fn buildRequestBody(
    allocator: std.mem.Allocator,
    model: []const u8,
    messages: []const p.Message,
    tools: []const p.Tool,
) ![]u8 {
    // Build the intermediate Value tree in an arena, then stringify into
    // the caller's allocator. The arena frees the tree on exit.
    var arena = std.heap.ArenaAllocator.init(allocator);
    defer arena.deinit();
    const a = arena.allocator();

    var root = std.json.ObjectMap.init(a);
    try root.put("model", .{ .string = model });
    try root.put("stream", .{ .bool = false });

    var msgs_arr = std.json.Array.init(a);
    for (messages) |msg| {
        try msgs_arr.append(try serializeMessage(a, msg));
    }
    try root.put("messages", .{ .array = msgs_arr });

    if (tools.len > 0) {
        var tools_arr = std.json.Array.init(a);
        for (tools) |tool| {
            var func_obj = std.json.ObjectMap.init(a);
            try func_obj.put("name", .{ .string = tool.name });
            try func_obj.put("description", .{ .string = tool.description });

            // Embed the schema as a parsed JSON value so it appears as an
            // object in the output, not as an escaped string.
            const schema_val = try std.json.parseFromSliceLeaky(
                std.json.Value,
                a,
                tool.input_schema,
                .{},
            );
            try func_obj.put("parameters", schema_val);

            var tool_obj = std.json.ObjectMap.init(a);
            try tool_obj.put("type", .{ .string = "function" });
            try tool_obj.put("function", .{ .object = func_obj });

            try tools_arr.append(.{ .object = tool_obj });
        }
        try root.put("tools", .{ .array = tools_arr });
    }

    return jsonStringifyAlloc(allocator, std.json.Value{ .object = root });
}

fn serializeMessage(a: std.mem.Allocator, msg: p.Message) !std.json.Value {
    var obj = std.json.ObjectMap.init(a);
    switch (msg) {
        .system => |text| {
            try obj.put("role", .{ .string = "system" });
            try obj.put("content", .{ .string = text });
        },
        .user => |text| {
            try obj.put("role", .{ .string = "user" });
            try obj.put("content", .{ .string = text });
        },
        .assistant => |content| {
            try obj.put("role", .{ .string = "assistant" });
            switch (content) {
                .text => |text| {
                    try obj.put("content", .{ .string = text });
                },
                .tool_calls => |calls| {
                    try obj.put("content", .null);

                    var tc_arr = std.json.Array.init(a);
                    for (calls) |tc| {
                        var tc_obj = std.json.ObjectMap.init(a);
                        try tc_obj.put("id", .{ .string = tc.id });
                        try tc_obj.put("type", .{ .string = "function" });

                        var func = std.json.ObjectMap.init(a);
                        try func.put("name", .{ .string = tc.name });
                        // OpenAI expects tool call arguments as a JSON-encoded string.
                        const args_str = try jsonStringifyAlloc(a, tc.input);
                        try func.put("arguments", .{ .string = args_str });

                        try tc_obj.put("function", .{ .object = func });
                        try tc_arr.append(.{ .object = tc_obj });
                    }
                    try obj.put("tool_calls", .{ .array = tc_arr });
                },
            }
        },
        .tool => |result| {
            try obj.put("role", .{ .string = "tool" });
            try obj.put("tool_call_id", .{ .string = result.call_id });
            // OpenAI expects tool results as a JSON-encoded string.
            const output_str = try jsonStringifyAlloc(a, result.output);
            try obj.put("content", .{ .string = output_str });
        },
    }
    return .{ .object = obj };
}

// ---------------------------------------------------------------------------
// Response parsing
// ---------------------------------------------------------------------------

fn parseResponse(allocator: std.mem.Allocator, body: []const u8) !p.Response {
    // `alloc_always` ensures all string slices in the tree are heap-allocated
    // in `allocator`, so they remain valid after this function returns.
    const root_val = try std.json.parseFromSliceLeaky(
        std.json.Value,
        allocator,
        body,
        .{ .allocate = .alloc_always },
    );

    if (root_val != .object) return error.InvalidResponse;
    const root = root_val.object;

    const choices_val = root.get("choices") orelse return error.InvalidResponse;
    if (choices_val != .array) return error.InvalidResponse;
    if (choices_val.array.items.len == 0) return error.InvalidResponse;

    const choice = choices_val.array.items[0];
    if (choice != .object) return error.InvalidResponse;

    const msg_val = choice.object.get("message") orelse return error.InvalidResponse;
    if (msg_val != .object) return error.InvalidResponse;
    const msg = msg_val.object;

    // Tool-call response
    if (msg.get("tool_calls")) |tc_val| {
        if (tc_val == .array and tc_val.array.items.len > 0) {
            var calls: std.ArrayList(p.ToolCall) = .{};
            for (tc_val.array.items) |tc_item| {
                if (tc_item != .object) continue;
                const tc = tc_item.object;

                const id_val = tc.get("id") orelse continue;
                const func_val = tc.get("function") orelse continue;
                if (func_val != .object) continue;
                const func = func_val.object;
                const name_val = func.get("name") orelse continue;
                const args_str_val = func.get("arguments") orelse continue;

                if (id_val != .string or name_val != .string or args_str_val != .string) continue;

                // Parse the arguments JSON string into a Value. Both the source
                // string and the resulting Value are owned by `allocator`.
                const args_input = try std.json.parseFromSliceLeaky(
                    std.json.Value,
                    allocator,
                    args_str_val.string,
                    .{ .allocate = .alloc_always },
                );

                try calls.append(allocator, .{
                    .id = id_val.string,
                    .name = name_val.string,
                    .input = args_input,
                });
            }
            return .{ .tool_calls = try calls.toOwnedSlice(allocator) };
        }
    }

    // Text response
    const content_val = msg.get("content") orelse return error.InvalidResponse;
    if (content_val != .string) return error.InvalidResponse;
    return .{ .text = content_val.string };
}
