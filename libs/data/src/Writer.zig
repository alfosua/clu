const Writer = @This();

const std = @import("std");

pub const DELIMITER = 0x00;

writer: *std.Io.Writer,

const HeaderPrefix = enum(u8) {
    response = 0x01,
    message = 0x02,
    reasoning = 0x03,
    tool_call = 0x04,
    tool_result = 0x05,
};

fn writeRaw(self: *Writer, data: u8) !void {
    try self.writer.writeAll(&[_]u8{data});
}

fn writeDelimiter(self: *Writer) !void {
    try self.writeRaw(DELIMITER);
}

pub fn write(self: *Writer, data: []const u8) !void {
    try self.writer.writeAll(data);
}

pub fn writeHeaderPrefix(self: *Writer, prefix: HeaderPrefix) !void {
    try self.writer.writeAll(&[_]u8{@intFromEnum(prefix)});
}

pub fn writeResponse(self: *Writer, response_id: []const u8) !void {
    try self.writeHeaderPrefix(.response);
    try self.write(response_id);
    try self.writeDelimiter();
}

pub fn beginMessage(self: *Writer) !void {
    try self.writeHeaderPrefix(.message);
}

pub fn endMessage(self: *Writer) !void {
    try self.writeDelimiter();
}

pub fn beginReasoning(self: *Writer) !void {
    try self.writeHeaderPrefix(.reasoning);
}

pub fn endReasoning(self: *Writer) !void {
    try self.writeDelimiter();
}

pub fn beginToolCall(self: *Writer, call_id: []const u8, tool_name: []const u8) !void {
    try self.writeHeaderPrefix(.tool_call);
    try self.write(call_id);
    try self.writeDelimiter();
    try self.write(tool_name);
    try self.writeDelimiter();
}

pub fn endToolCall(self: *Writer) !void {
    try self.writeDelimiter();
}

pub fn beginToolResult(self: *Writer, call_id: []const u8) !void {
    try self.writeHeaderPrefix(.tool_result);
    try self.write(call_id);
    try self.writeDelimiter();
}

pub fn endToolResult(self: *Writer) !void {
    try self.writeDelimiter();
}
