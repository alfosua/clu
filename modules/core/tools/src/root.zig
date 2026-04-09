const std = @import("std");

/// JSON value type used for tool input and output.
pub const Value = std.json.Value;

/// A tool call dispatched by the model.
pub const ToolCall = struct {
    /// Call ID assigned by the model, used to correlate results.
    id: []const u8,
    /// Name of the tool to invoke.
    name: []const u8,
    /// Input arguments as a JSON object.
    input: Value,
};

/// The result of a tool call, appended to the message history.
pub const ToolResult = struct {
    /// The call ID this result corresponds to.
    call_id: []const u8,
    /// The tool's output as a JSON value.
    output: Value,
};

/// A tool the agent can invoke.
///
/// Implementations wrap a concrete type behind an opaque pointer and expose
/// it through this vtable struct, matching the pattern used by
/// `std.mem.Allocator`.
pub const Tool = struct {
    /// Unique identifier sent to the model (e.g. `"bash"`).
    name: []const u8,
    /// Natural language description sent to the model.
    description: []const u8,
    /// JSON Schema describing the tool's parameters, serialised as a string.
    input_schema: []const u8,
    /// Opaque pointer to the concrete implementation.
    ptr: *anyopaque,
    /// Execution function. `input` is a JSON object matching `input_schema`;
    /// the returned `Value` is the tool's output.
    callFn: *const fn (
        ptr: *anyopaque,
        allocator: std.mem.Allocator,
        input: Value,
    ) anyerror!Value,

    pub fn call(self: Tool, allocator: std.mem.Allocator, input: Value) !Value {
        return self.callFn(self.ptr, allocator, input);
    }
};
