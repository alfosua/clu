//! By convention, root.zig is the root source file when making a package.
const std = @import("std");
const Io = std.Io;

pub const commons = @import("commons.zig");
pub const GlobalInit = commons.GlobalInit;

pub fn loop(init: GlobalInit) !void {
    const reader = init.reader;
    const writer = init.writer;

    while (try reader.takeDelimiter('\n')) |line| {
        try writer.writeAll("Processing...\n");
        try writer.flush();

        var client = std.http.Client{ .allocator = init.arena, .io = init.io };

        var payloadWriterAlloc = std.Io.Writer.Allocating.init(init.gpa);
        defer payloadWriterAlloc.deinit();
        const payloadWriter = &payloadWriterAlloc.writer;

        try std.json.Stringify.value(.{
            .model = "nemotron-3-nano:4b",
            .input = .{
                .{ .role = "user", .content = line },
            },
        }, .{}, payloadWriter);

        var bodyWriterAlloc = std.Io.Writer.Allocating.init(init.gpa);
        defer bodyWriterAlloc.deinit();
        const bodyWriter = &bodyWriterAlloc.writer;

        const uri = try std.Uri.parse("http://localhost:11434/v1/responses");
        _ = try client.fetch(.{
            .method = .POST,
            .location = .{ .uri = uri },
            .headers = .{
                .content_type = .{ .override = "application/json; charset=utf-8" },
                .authorization = .{ .override = "Bearer ollama" },
            },
            .response_writer = bodyWriter,
            .payload = payloadWriter.toArrayList().items,
        });

        const body = bodyWriter.toArrayList().items;
        //std.debug.print("Received response: {s}\n", .{body});
        const bodyJson = try std.json.parseFromSlice(std.json.Value, init.arena, body, .{});
        const result = bodyJson.value;

        if (result != .object) {
            try writer.writeAll("Unexpected response format\n");
            try writer.flush();
            continue;
        }

        const output = result.object.get("output") orelse {
            try writer.writeAll("No output in response\n");
            try writer.flush();
            continue;
        };

        for (output.array.items) |item| {
            const itemType = item.object.get("type") orelse continue;

            if (std.mem.eql(u8, itemType.string, "message")) {
                const content = item.object.get("content") orelse continue;

                for (content.array.items) |element| {
                    const text = element.object.get("text") orelse continue;
                    try writer.print("{s}", .{text.string});
                    try writer.writeAll("\n");
                }
            }
        }
        try writer.flush();
    }
}
