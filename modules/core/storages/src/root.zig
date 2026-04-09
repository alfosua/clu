const std = @import("std");
const p = @import("core_providers");

pub const Message = p.Message;

/// A session storage backend.
///
/// Defines how message history is persisted within and optionally across
/// sessions. Implementations live in `modules/storage/`.
pub const Storage = struct {
    ptr: *anyopaque,
    appendFn: *const fn (ptr: *anyopaque, message: Message) anyerror!void,
    historyFn: *const fn (
        ptr: *anyopaque,
        allocator: std.mem.Allocator,
    ) anyerror![]Message,
    clearFn: *const fn (ptr: *anyopaque) anyerror!void,

    /// Append a message to history.
    pub fn append(self: Storage, message: Message) !void {
        return self.appendFn(self.ptr, message);
    }

    /// Retrieve the full message history. Caller owns the returned slice.
    pub fn history(self: Storage, allocator: std.mem.Allocator) ![]Message {
        return self.historyFn(self.ptr, allocator);
    }

    /// Reset history, discarding all messages.
    pub fn clear(self: Storage) !void {
        return self.clearFn(self.ptr);
    }
};
