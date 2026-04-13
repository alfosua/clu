const std = @import("std");
const t = @import("core_tools");

const input_schema =
    \\{
    \\  "type": "object",
    \\  "properties": {
    \\    "command": { "type": "string", "description": "The PowerShell command to run." }
    \\  },
    \\  "required": ["command"]
    \\}
;

pub const PwshTool = struct {
    /// Return a `Tool` vtable backed by this instance.
    /// The caller must keep `self` alive for the lifetime of the returned `Tool`.
    pub fn tool(self: *PwshTool) t.Tool {
        return .{
            .name = "pwsh",
            .description = "Execute a PowerShell command and return its output.",
            .input_schema = input_schema,
            .ptr = self,
            .callFn = callFn,
        };
    }

    fn callFn(ptr: *anyopaque, allocator: std.mem.Allocator, input: t.Value) anyerror!t.Value {
        _ = ptr;

        const command = switch (input) {
            .object => |obj| blk: {
                const cmd_val = obj.get("command") orelse return error.MissingCommand;
                break :blk switch (cmd_val) {
                    .string => |s| s,
                    else => return error.InvalidCommand,
                };
            },
            else => return error.InvalidInput,
        };

        const result = try std.process.Child.run(.{
            .allocator = allocator,
            .argv = &.{ "pwsh", "-Command", command },
            .max_output_bytes = 4 * 1024 * 1024,
        });
        // result.stdout and result.stderr are allocated by `allocator`;
        // transfer ownership into the returned Value (no dupe needed).

        const exit_code: i64 = switch (result.term) {
            .Exited => |code| @intCast(code),
            .Signal => |sig| -@as(i64, @intCast(sig)),
            .Stopped => |sig| -@as(i64, @intCast(sig)),
            .Unknown => |code| @intCast(code),
        };

        var obj = std.json.ObjectMap.init(allocator);
        try obj.put("stdout", .{ .string = result.stdout });
        try obj.put("stderr", .{ .string = result.stderr });
        try obj.put("exit_code", .{ .integer = exit_code });
        return t.Value{ .object = obj };
    }
};
