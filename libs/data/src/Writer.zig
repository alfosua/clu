const Writer = @This();

const std = @import("std");

writer: *std.Io.Writer,

pub const Prefix = enum(u8) {
    delimiter = 0x00,
    send,
    response,
    message,
    reasoning,
    tool_call,
    tool_result,
};

fn writeRaw(self: *Writer, data: u8) !void {
    try self.writer.writeAll(&[_]u8{data});
}

pub fn write(self: *Writer, data: []const u8) !void {
    try self.writer.writeAll(data);
}

pub fn writeHeaderPrefix(self: *Writer, prefix: Prefix) !void {
    try self.writer.writeAll(&[_]u8{@intFromEnum(prefix)});
}

fn writeDelimiter(self: *Writer) !void {
    try self.writeHeaderPrefix(.delimiter);
}

pub fn writeResponse(self: *Writer, response_id: []const u8) !void {
    try self.writeHeaderPrefix(.response);
    try self.write(response_id);
    try self.writeDelimiter();
}

pub fn beginMessage(self: *Writer) !void {
    try self.writeHeaderPrefix(.message);
}

pub fn beginReasoning(self: *Writer) !void {
    try self.writeHeaderPrefix(.reasoning);
}

pub fn beginToolCall(self: *Writer, call_id: []const u8, tool_name: []const u8) !void {
    try self.writeHeaderPrefix(.tool_call);
    try self.write(call_id);
    try self.writeDelimiter();
    try self.write(tool_name);
    try self.writeDelimiter();
}

pub fn beginToolResult(self: *Writer, call_id: []const u8) !void {
    try self.writeHeaderPrefix(.tool_result);
    try self.write(call_id);
    try self.writeDelimiter();
}

pub fn endBlock(self: *Writer) !void {
    try self.writeDelimiter();
}
