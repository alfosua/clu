const std = @import("std");

pub fn main(init: std.process.Init) !void {
    var stdout_buffer: [1024 * 1024]u8 = undefined;
    var stdout_wrapper = std.Io.File.stdout().writer(init.io, &stdout_buffer);
    const stdout_writer = &stdout_wrapper.interface;

    var describe = false;
    var path: []const u8 = undefined;
    var path_given = false;
    var args = init.minimal.args.iterate();
    while (args.next()) |arg| {
        if (std.mem.eql(u8, arg, "--describe")) {
            describe = true;
        } else if (std.mem.eql(u8, arg, "--path")) {
            path = args.next() orelse {
                return error.MissingValue;
            };
            path_given = true;
        }
    }

    if (describe) {
        var jws = std.json.Stringify{
            .writer = stdout_writer,
            //.indent_level = 2,
        };
        try jws.beginObject();

        try jws.objectField("name");
        try jws.write("read");

        try jws.objectField("description");
        try jws.write("Read the content of a file given its path on the file system.");

        try jws.objectField("parameters");
        try jws.beginObject();

        try jws.objectField("type");
        try jws.write("object");

        try jws.objectField("properties");
        try jws.beginObject();

        try jws.objectField("path");
        try jws.beginObject();
        try jws.objectField("type");
        try jws.write("string");
        try jws.endObject();

        try jws.endObject(); // end properties

        try jws.objectField("required");
        try jws.beginArray();
        try jws.write("path");
        try jws.endArray();

        try jws.endObject(); // end parameters

        try jws.endObject(); // end root

        try stdout_writer.flush();

        return;
    }

    if (!path_given or path.len == 0) {
        return error.MissingPath;
    }

    const file = try std.Io.Dir.cwd().openFile(init.io, path, .{
        .mode = .read_only,
    });

    var read_buffer: [1024 * 1024 * 4]u8 = undefined;
    var file_wrapper = file.reader(init.io, &read_buffer);
    const file_reader = &file_wrapper.interface;

    _ = try file_reader.streamRemaining(stdout_writer);

    try stdout_writer.flush();
}
