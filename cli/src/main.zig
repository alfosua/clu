const std = @import("std");

const Options = struct {
    provider: ?[]const u8 = null,
};

pub fn main(init: std.process.Init) !void {
    var args = init.minimal.args.iterate();
    var opts = Options{};

    while (args.next()) |arg| {
        if (std.mem.eql(u8, arg, "--provider") or std.mem.eql(u8, arg, "-p")) {
            opts.provider = args.next() orelse {
                std.log.err("Missing value for '{s}'", .{arg});
                return error.MissingArgument;
            };
        }
    }
}

