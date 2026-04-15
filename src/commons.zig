const std = @import("std");
const Allocator = std.mem.Allocator;
const Writer = std.Io.Writer;
const Reader = std.Io.Reader;

pub const GlobalInit = struct {
    gpa: Allocator,
    arena: Allocator,
    io: std.Io,
    writer: *Writer,
    reader: *Reader,
};
