const std = @import("std");

const Options = struct {
    prompt: []const u8 = undefined,
    baseUrl: []const u8 = undefined,
    apiKey: []const u8 = undefined,
    model: []const u8 = undefined,
    tools: []const u8 = undefined,
};

pub fn main(init: std.process.Init) !void {
    const opts = try loadOptions(init);
    const allocator = init.arena.allocator();

    var client = std.http.Client{ .allocator = allocator, .io = init.io };

    const uri = try createResponsesUri(allocator, opts.baseUrl);

    const parts = &[_][]const u8{ "Bearer ", opts.apiKey };
    const authHeaderValue = try std.mem.concat(allocator, u8, parts);

    var req = try client.request(.POST, uri, .{
        .headers = .{
            .authorization = .{ .override = authHeaderValue },
        },
    });

    const req_body_buffer = try createRequestBodyBuffer(allocator, opts);
    try req.sendBodyComplete(req_body_buffer);

    var response_buffer: [1024]u8 = undefined;
    var response = try req.receiveHead(&response_buffer);

    if (response.head.status != .ok) {
        return error.UnexpectedError;
    }

    var body_buffer: [1024 * 1024]u8 = undefined;
    const body_reader = response.reader(&body_buffer);

    // Initialize standard output
    var stdout_buffer: [1024 * 1024]u8 = undefined;
    var stdout_wrapper = std.Io.File.stdout().writer(init.io, &stdout_buffer);
    const stdout = &stdout_wrapper.interface;

    const response_ctx = ResponseContext{
        .reader = body_reader,
        .writer = stdout,
        .allocator = allocator,
    };

    try processResponse(response_ctx);
}

fn createRequestBodyBuffer(
    allocator: std.mem.Allocator,
    options: Options,
) ![]u8 {
    var json = std.Io.Writer.Allocating.init(allocator);
    var jws = std.json.Stringify{
        .writer = &json.writer,
        .options = .{},
    };
    try jws.beginObject();

    try jws.objectField("model");
    try jws.write(options.model);

    try jws.objectField("input");
    try jws.write(options.prompt);

    try jws.objectField("stream");
    try jws.write(true);

    try jws.objectField("tools");
    try jws.beginWriteRaw();
    _ = try json.writer.write(options.tools);
    jws.endWriteRaw();

    try jws.endObject();

    return json.writer.buffered();
}

const ResponseContext = struct {
    reader: *std.Io.Reader,
    writer: *std.Io.Writer,
    allocator: std.mem.Allocator,
};

fn processResponse(ctx: ResponseContext) !void {
    while (true) {
        const line = try ctx.reader.takeDelimiter('\n') orelse break;
        if (line.len == 0) continue;
        if (!std.mem.startsWith(u8, line, "event: ")) continue;

        const next_line = try ctx.reader.takeDelimiter('\n') orelse break;
        if (next_line.len == 0) continue;
        if (!std.mem.startsWith(u8, next_line, "data: ")) continue;

        const event_str = line[7..];
        const event = eventMap.get(event_str) orelse .unknown;
        const data = next_line[6..];

        const eventCtx = EventContext{
            .event = event,
            .data = data,
            .writer = ctx.writer,
            .allocator = ctx.allocator,
        };
        const handler = eventHandlerMap.get(event);

        try handler(eventCtx);
    }
}

const Event = enum {
    response_created,
    response_completed,
    response_in_progress,
    response_output_item_added,
    response_reasoning_summary_text_delta,
    response_reasoning_summary_text_done,
    response_output_item_done,
    response_output_text_delta,
    response_output_text_done,
    response_content_part_added,
    response_content_part_done,
    response_function_call_arguments_delta,
    response_function_call_arguments_done,
    unknown,
};
const eventMap = std.StaticStringMap(Event).initComptime(.{
    .{ "response.created", .response_created },
    .{ "response.in_progress", .response_in_progress },
    .{ "response.completed", .response_completed },
    .{ "response.output_item.added", .response_output_item_added },
    .{ "response.reasoning_summary_text.delta", .response_reasoning_summary_text_delta },
    .{ "response.reasoning_summary_text.done", .response_reasoning_summary_text_done },
    .{ "response.output_item.done", .response_output_item_done },
    .{ "response.output_text.delta", .response_output_text_delta },
    .{ "response.output_text.done", .response_output_text_done },
    .{ "response.content_part.added", .response_content_part_added },
    .{ "response.content_part.done", .response_content_part_done },
    .{ "response.function_call_arguments.delta", .response_function_call_arguments_delta },
    .{ "response.function_call_arguments.done", .response_function_call_arguments_done },
});
const EventContext = struct {
    event: Event,
    data: []const u8,
    writer: *std.Io.Writer,
    allocator: std.mem.Allocator,
};
const EventHandlerError = error{
    InvalidOperation,
};
const EventHandlerFn = *const fn (EventContext) EventHandlerError!void;
const eventHandlerMap = std.EnumArray(Event, EventHandlerFn).init(.{
    .response_created = handleResponseCreatedEvent,
    .response_in_progress = handleEventToNull,
    .response_completed = handleEventToNull,
    .response_output_item_added = handleResponseOutputItemAdded,
    .response_reasoning_summary_text_delta = handleResponseDelta,
    .response_reasoning_summary_text_done = handleEventToNull,
    .response_output_item_done = handleResponseOutputItemDone,
    .response_output_text_delta = handleResponseDelta,
    .response_output_text_done = handleEventToNull,
    .response_content_part_added = handleEventToNull,
    .response_content_part_done = handleEventToNull,
    .response_function_call_arguments_delta = handleResponseDelta,
    .response_function_call_arguments_done = handleEventToNull,
    .unknown = handleEventToLog,
});

fn handleEventToNull(_: EventContext) EventHandlerError!void {}

fn handleEventToLog(ctx: EventContext) EventHandlerError!void {
    std.log.info("Event {s}: {s}", .{ @tagName(ctx.event), ctx.data });
}

fn handleResponseCompletedEvent(ctx: EventContext) EventHandlerError!void {
    try handleEventToLog(ctx);
}

fn parseJson(allocator: std.mem.Allocator, s: []const u8, options: std.json.ParseOptions) std.json.ParseError(std.json.Scanner)!std.json.Parsed(std.json.Value) {
    return std.json.parseFromSlice(std.json.Value, allocator, s, options);
}

fn handleResponseCreatedEvent(ctx: EventContext) EventHandlerError!void {
    const json = parseJson(ctx.allocator, ctx.data, .{}) catch |err| {
        std.log.err("Error parsing json: {s}", .{@errorName(err)});
        return EventHandlerError.InvalidOperation;
    };
    const response = json.value.object.get("response") orelse {
        std.log.err("Bad model", .{});
        return EventHandlerError.InvalidOperation;
    };
    const id = response.object.get("id") orelse {
        std.log.err("No ID defined", .{});
        return EventHandlerError.InvalidOperation;
    };
    writeResponseStarter(ctx.writer, id.string) catch |err| {
        std.log.err("Failed to write: {s}", .{@errorName(err)});
        return EventHandlerError.InvalidOperation;
    };
}

fn writeResponseStarter(writer: *std.Io.Writer, id: []const u8) !void {
    try writer.writeAll("<response id=\"");
    try writer.writeAll(id);
    try writer.writeAll("\">");
    try writer.writeAll("\n");
    try writer.flush();
}

fn handleResponseOutputItemAdded(ctx: EventContext) EventHandlerError!void {
    const json = parseJson(ctx.allocator, ctx.data, .{}) catch |err| {
        std.log.err("Error parsing json: {s}", .{@errorName(err)});
        return EventHandlerError.InvalidOperation;
    };
    const item = json.value.object.get("item") orelse {
        std.log.err("Bad model", .{});
        return EventHandlerError.InvalidOperation;
    };
    const item_id = item.object.get("id") orelse {
        std.log.err("No ID defined", .{});
        return EventHandlerError.InvalidOperation;
    };
    const item_type_json = item.object.get("type") orelse {
        std.log.err("No type defined", .{});
        return EventHandlerError.InvalidOperation;
    };
    const item_type = std.meta.stringToEnum(ResponseOutputItemType, item_type_json.string) orelse return EventHandlerError.InvalidOperation;
    if (item_type == .function_call) {
        const call_id = item.object.get("call_id") orelse {
            std.log.err("No call id defined", .{});
            return EventHandlerError.InvalidOperation;
        };
        const name = item.object.get("name") orelse {
            std.log.err("No name defined", .{});
            return EventHandlerError.InvalidOperation;
        };
        writeFunctionCallStarter(
            ctx.writer,
            item_id.string,
            call_id.string,
            name.string,
        ) catch |err| {
            std.log.err("Failed to write: {s}", .{@errorName(err)});
            return EventHandlerError.InvalidOperation;
        };
    } else {
        writeOutputStarter(ctx.writer, item_id.string, item_type) catch |err| {
            std.log.err("Failed to write: {s}", .{@errorName(err)});
            return EventHandlerError.InvalidOperation;
        };
    }
}

const ResponseOutputItemType = enum {
    reasoning,
    message,
    function_call,
};

fn writeOutputStarter(writer: *std.Io.Writer, id: []const u8, item_type: ResponseOutputItemType) !void {
    try writer.writeAll("<");
    try writer.writeAll(@tagName(item_type));
    try writer.writeAll(" id=\"");
    try writer.writeAll(id);
    try writer.writeAll("\">");
    try writer.writeAll("\n");
    try writer.flush();
}

fn writeFunctionCallStarter(
    writer: *std.Io.Writer,
    id: []const u8,
    call_id: []const u8,
    name: []const u8,
) !void {
    try writer.writeAll("<function-call ");
    try writer.writeAll(" id=\"");
    try writer.writeAll(id);
    try writer.writeAll("\"");
    try writer.writeAll(" call-id=\"");
    try writer.writeAll(call_id);
    try writer.writeAll("\"");
    try writer.writeAll(" name=\"");
    try writer.writeAll(name);
    try writer.writeAll("\">");
    try writer.flush();
}

fn handleResponseOutputItemDone(ctx: EventContext) EventHandlerError!void {
    const json = parseJson(ctx.allocator, ctx.data, .{}) catch |err| {
        std.log.err("Error parsing json: {s}", .{@errorName(err)});
        return EventHandlerError.InvalidOperation;
    };
    const item = json.value.object.get("item") orelse {
        std.log.err("Bad model", .{});
        return EventHandlerError.InvalidOperation;
    };
    const item_type_json = item.object.get("type") orelse {
        std.log.err("No type defined", .{});
        return EventHandlerError.InvalidOperation;
    };
    const item_type = std.meta.stringToEnum(ResponseOutputItemType, item_type_json.string) orelse return EventHandlerError.InvalidOperation;
    writeOutputItemFinalizer(ctx.writer, item_type) catch |err| {
        std.log.err("Failed to write: {s}", .{@errorName(err)});
        return EventHandlerError.InvalidOperation;
    };
}

fn writeOutputItemFinalizer(writer: *std.Io.Writer, item_type: ResponseOutputItemType) !void {
    if (item_type == .function_call) {
        try writer.writeAll("</function-call>\n");
    } else {
        try writer.writeAll("\n");
        try writer.writeAll("</");
        try writer.writeAll(@tagName(item_type));
        try writer.writeAll(">");
        try writer.writeAll("\n");
        try writer.flush();
    }
}

fn handleResponseDelta(ctx: EventContext) EventHandlerError!void {
    const json = parseJson(ctx.allocator, ctx.data, .{}) catch |err| {
        std.log.err("Error parsing json: {s}", .{@errorName(err)});
        return EventHandlerError.InvalidOperation;
    };
    const delta = json.value.object.get("delta") orelse {
        std.log.err("No delta defined", .{});
        return EventHandlerError.InvalidOperation;
    };
    // const item_id = json.object.get("item_id") orelse {
    //    std.log.err("No ID defined", .{});
    //    return EventHandlerError.InvalidOperation;
    // };
    writeDelta(ctx.writer, delta.string) catch |err| {
        std.log.err("Failed to write: {s}", .{@errorName(err)});
        return EventHandlerError.InvalidOperation;
    };
}

fn writeDelta(writer: *std.Io.Writer, delta: []const u8) !void {
    try writer.writeAll(delta);
    try writer.flush();
}

const OptionDef = struct {
    alias: []const u8,
    short_alias: []const u8,
};

const BASE_URL_OPT = OptionDef{ .alias = "--base-url", .short_alias = "-u" };
const API_KEY_OPT = OptionDef{ .alias = "--api-key", .short_alias = "-k" };
const MODEL_OPT = OptionDef{ .alias = "--model", .short_alias = "-m" };

fn loadOptions(init: std.process.Init) ArgParseError!Options {
    var args = init.minimal.args.iterate();

    var prompt: ?[]const u8 = null;
    var baseUrl: ?[]const u8 = null;
    var apiKey: ?[]const u8 = null;
    var model: ?[]const u8 = null;
    const tools = init.environ_map.get("CLU_AVAILABLE_TOOLS") orelse "[]";

    while (args.next()) |arg| {
        if (isOption(arg, BASE_URL_OPT)) {
            baseUrl = args.next() orelse return missingValueError(arg);
        } else if (isOption(arg, API_KEY_OPT)) {
            apiKey = args.next() orelse return missingValueError(arg);
        } else if (isOption(arg, MODEL_OPT)) {
            model = args.next() orelse return missingValueError(arg);
        } else {
            prompt = arg;
        }
    }

    return Options{
        .prompt = prompt orelse return missingArgumentError("PROMPT"),
        .baseUrl = baseUrl orelse return missingRequiredOptionError("--base-url"),
        .apiKey = apiKey orelse return missingRequiredOptionError("--api-key"),
        .model = model orelse return missingRequiredOptionError("--model"),
        .tools = tools,
    };
}

fn isOption(arg: []const u8, option: OptionDef) bool {
    return std.mem.eql(u8, arg, option.short_alias) or std.mem.eql(u8, arg, option.alias);
}

const ArgParseError = error{
    MissingArgument,
    MissingRequiredOption,
    MissingValue,
};

fn missingArgumentError(arg: []const u8) ArgParseError {
    std.log.err("Missing argument: {s}", .{
        arg,
    });
    return error.MissingArgument;
}

fn missingRequiredOptionError(arg: []const u8) ArgParseError {
    std.log.err("Missing required option: {s}", .{
        arg,
    });
    return error.MissingRequiredOption;
}

fn missingValueError(arg: []const u8) ArgParseError {
    std.log.err("Missing value of '{s}'", .{
        arg,
    });
    return error.MissingValue;
}

fn createResponsesUri(
    allocator: std.mem.Allocator,
    baseUrl: []const u8,
) !std.Uri {
    const arr = &[_][]const u8{ baseUrl, "/responses" };
    const url = try std.mem.concat(allocator, u8, arr);
    return try std.Uri.parse(url);
}
