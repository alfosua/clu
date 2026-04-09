const t = @import("core_tools");

pub const Tool = t.Tool;

/// Opaque handle to the running agent session passed to lifecycle hooks.
/// The agent casts this to `*Agent` at call sites; extensions treat it as
/// an untyped context token to avoid a circular dependency on `core/agent`.
pub const SessionHandle = *anyopaque;

/// An extension — a bundle of tools and/or lifecycle hooks that can be
/// registered into a session. Extensions are the composition unit for
/// capabilities that need setup, teardown, or config injection beyond what
/// a bare `Tool` provides.
pub const Extension = struct {
    ptr: *anyopaque,
    /// Return the tools this extension contributes to the session.
    toolsFn: *const fn (ptr: *anyopaque) []const Tool,
    /// Called once when a session begins, before the first agent turn.
    onSessionStartFn: *const fn (
        ptr: *anyopaque,
        session: SessionHandle,
    ) anyerror!void,
    /// Called once when a session ends, after the last agent turn.
    onSessionEndFn: *const fn (
        ptr: *anyopaque,
        session: SessionHandle,
    ) anyerror!void,

    pub fn tools(self: Extension) []const Tool {
        return self.toolsFn(self.ptr);
    }

    pub fn onSessionStart(self: Extension, session: SessionHandle) !void {
        return self.onSessionStartFn(self.ptr, session);
    }

    pub fn onSessionEnd(self: Extension, session: SessionHandle) !void {
        return self.onSessionEndFn(self.ptr, session);
    }
};
