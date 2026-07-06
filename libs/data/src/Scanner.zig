const Scanner = @This();

const std = @import("std");
const Prefix = @import("Writer.zig").Prefix;
const DELIMITER_U8 = @intFromEnum(Prefix.delimiter);

reader: *std.Io.Reader,
is_building_body: bool = false,

const IdentifiedToken = struct {
    id: []const u8,
};
const ToolCallToken = struct {
    id: []const u8,
    call_id: []const u8,
    name: []const u8,
};
const Token = union(enum) {
    delimiter,
    delta: []const u8,
    response: IdentifiedToken,
    message: IdentifiedToken,
    reasoning: IdentifiedToken,
    tool_call: ToolCallToken,
    tool_result: IdentifiedToken,
};

pub fn init(reader: *std.Io.Reader) Scanner {
    return Scanner{ .reader = reader };
}

pub fn next(self: *Scanner) !?Token {
    if (self.is_building_body) {
        return self.nextBody();
    } else {
        return self.nextHeader();
    }
}

fn nextHeader(self: *Scanner) !Token {
    const byte = try self.reader.takeByte();
    const prefix: Prefix = @enumFromInt(byte);
    switch (prefix) {
        .delimiter => return .delimiter,
        .response or .message or .reasoning or .tool_result => return try buildIdentifiedToken(self.reader),
        .tool_call => return try buildToolCallToken(self.reader),
    }
}

fn nextBody(self: *Scanner) !Token {
    while (true) {
        const buf = self.reader.buffered();

        if (buf.len > 0) {
            if (std.mem.indexOfScalar(u8, buf, DELIMITER_U8)) |idx| {
                const chunk = buf[0..idx];
                self.reader.toss(idx + 1);
                self.is_building_body = false;
                return .{ .delta = chunk };
            }
            const chunk = buf[0..];
            self.reader.toss(buf.len);
            return .{ .delta = chunk };
        }

        self.reader.fillMore() catch |err| switch (err) {
            error.EndOfStream => return .delimiter,
            else => return err,
        };
    }
}

fn buildIdentifiedToken(reader: *std.Io.Reader) !IdentifiedToken {
    const id = try reader.takeDelimiter(DELIMITER_U8);
    return .{ .id = id };
}

fn buildToolCallToken(reader: *std.Io.Reader) !ToolCallToken {
    const id = try reader.takeDelimiter(DELIMITER_U8);
    const call_id = try reader.takeDelimiter(DELIMITER_U8);
    const name = try reader.takeDelimiter(DELIMITER_U8);
    return .{ .id = id, .call_id = call_id, .name = name };
}
