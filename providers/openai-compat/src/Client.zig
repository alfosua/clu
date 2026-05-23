const Client = @This();

const std = @import("std");

allocator: std.mem.Allocator,
io: std.Io,
options: Options,
http_client: std.http.Client,
http_uri: std.Uri,
http_headers: std.http.Client.Request.Headers,

pub fn init(allocator: std.mem.Allocator, io: std.Io, options: Options) !Client {
    const http_client = std.http.Client{ .allocator = allocator, .io = io };
    const auth_header_value = try buildAuthHeaderValue(allocator, options.api_key);
    std.log.err("{s}", .{options.endpoint});
    return Client{
        .allocator = allocator,
        .io = io,
        .options = options,
        .http_client = http_client,
        .http_uri = try std.Uri.parse(options.endpoint),
        .http_headers = .{ .authorization = .{ .override = auth_header_value }, .content_type = .{ .override = "application/json" } },
    };
}

fn buildAuthHeaderValue(allocator: std.mem.Allocator, api_key: []const u8) ![]const u8 {
    const parts = &[_][]const u8{ "Bearer ", api_key };
    return try std.mem.concat(allocator, u8, parts);
}

pub fn request(self: *Client) !Request {
    var http_request = try self.http_client.request(.POST, self.http_uri, .{
        .headers = self.http_headers,
    });
    http_request.transfer_encoding = .chunked;
    var body_writer = try http_request.sendBodyUnflushed(&.{});

    var req = Request{ .http_request = http_request, .body_writer = body_writer, .jws = .{ .writer = &body_writer.writer } };
    try req.begin();
    return req;
}

const Options = struct {
    endpoint: []const u8,
    api_key: []const u8,
};

const Request = struct {
    http_request: std.http.Client.Request,
    body_writer: std.http.BodyWriter,
    jws: std.json.Stringify,

    pub fn begin(self: *Request) !void {
        try self.jws.beginObject();
        try self.jws.objectField("stream");
        try self.jws.write(true);
    }

    pub fn end(self: *Request) !void {
        try self.jws.endObject();
    }

    pub fn send(self: *Request) !void {
        try self.end();
        try self.body_writer.flush();
        try self.body_writer.end();
    }

    pub fn receive(self: *Request, transfer_buffer: []u8) !*std.Io.Reader {
        var redirect_buffer: [4096]u8 = undefined;
        var http_response = try self.http_request.receiveHead(&redirect_buffer);
        if (http_response.head.status != .ok) {
            std.log.err("Status: {d} {s}", .{ @intFromEnum(http_response.head.status), @tagName(http_response.head.status) });
            return error.BadRequest;
        }
        const reader = http_response.reader(transfer_buffer);
        return reader;
    }

    pub fn model(self: *Request, m: []const u8) !void {
        try self.jws.objectField("model");
        try self.jws.write(m);
    }

    pub fn toolsArray(self: *Request, tools: []const u8) !void {
        try self.jws.objectField("tools");
        try self.jws.beginWriteRaw();
        try self.jws.writer.writeAll(tools);
        self.jws.endWriteRaw();
    }

    pub fn beginInput(self: *Request) !void {
        try self.jws.objectField("input");
        try self.jws.beginWriteRaw();
        try self.jws.writer.writeByte('"');
    }

    pub fn writeDelta(self: *Request, delta: []const u8) !void {
        try std.json.Stringify.encodeJsonStringChars(delta, .{}, self.jws.writer);
    }

    pub fn endInput(self: *Request) !void {
        try self.jws.writer.writeByte('"');
        self.jws.endWriteRaw();
    }
};
