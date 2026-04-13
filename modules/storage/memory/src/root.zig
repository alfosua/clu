const std = @import("std");
const s = @import("core_storages");

pub const MemoryStorage = struct {
    allocator: std.mem.Allocator,
    messages: std.ArrayList(s.Message),

    pub fn init(allocator: std.mem.Allocator) MemoryStorage {
        return .{
            .allocator = allocator,
            .messages = .{},
        };
    }

    pub fn deinit(self: *MemoryStorage) void {
        self.messages.deinit(self.allocator);
    }

    /// Return a `Storage` vtable backed by this instance.
    /// The caller must keep `self` alive for the lifetime of the returned `Storage`.
    pub fn storage(self: *MemoryStorage) s.Storage {
        return .{
            .ptr = self,
            .appendFn = appendFn,
            .historyFn = historyFn,
            .clearFn = clearFn,
        };
    }

    fn appendFn(ptr: *anyopaque, message: s.Message) anyerror!void {
        const self: *MemoryStorage = @ptrCast(@alignCast(ptr));
        try self.messages.append(self.allocator, message);
    }

    fn historyFn(ptr: *anyopaque, allocator: std.mem.Allocator) anyerror![]s.Message {
        const self: *MemoryStorage = @ptrCast(@alignCast(ptr));
        return try allocator.dupe(s.Message, self.messages.items);
    }

    fn clearFn(ptr: *anyopaque) anyerror!void {
        const self: *MemoryStorage = @ptrCast(@alignCast(ptr));
        self.messages.clearRetainingCapacity();
    }
};
