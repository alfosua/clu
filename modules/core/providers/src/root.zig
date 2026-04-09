const std = @import("std");
const t = @import("core_tools");

pub const Tool = t.Tool;
pub const ToolCall = t.ToolCall;
pub const ToolResult = t.ToolResult;
pub const Value = t.Value;

/// The role of a conversation participant.
pub const Role = enum { system, user, assistant, tool };

/// Content of an assistant turn: either a text reply or a list of tool calls.
pub const AssistantContent = union(enum) {
    text: []const u8,
    tool_calls: []const ToolCall,
};

/// A single message in the conversation history.
pub const Message = union(Role) {
    system: []const u8,
    user: []const u8,
    assistant: AssistantContent,
    tool: ToolResult,
};

/// The result returned by a provider completion call.
pub const Response = union(enum) {
    /// The model produced a text reply and the turn is complete.
    text: []const u8,
    /// The model requested one or more tool invocations.
    tool_calls: []const ToolCall,
};

/// An LLM provider.
///
/// Wraps a concrete implementation behind an opaque pointer. The `complete`
/// function sends the full message history and the registered tool schemas,
/// then returns either a text reply or a list of tool calls to dispatch.
pub const Provider = struct {
    ptr: *anyopaque,
    completeFn: *const fn (
        ptr: *anyopaque,
        allocator: std.mem.Allocator,
        messages: []const Message,
        tools: []const Tool,
    ) anyerror!Response,

    pub fn complete(
        self: Provider,
        allocator: std.mem.Allocator,
        messages: []const Message,
        tools: []const Tool,
    ) !Response {
        return self.completeFn(self.ptr, allocator, messages, tools);
    }
};
