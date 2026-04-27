const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    const exes = [_][]const u8{
        "clu",
        "clu_loop_std_single_thread_sync",
        "clu_provider_openai_compat",
        "clu_tool_std_read",
        "clu_tool_std_ls",
    };

    for (exes) |exe_name| {
        const dep = b.dependency(exe_name, .{
            .target = target,
            .optimize = optimize,
        });

        const exe = dep.artifact(exe_name);
        b.installArtifact(exe);

        const run_step_info = createRunStepInfo(b.allocator, exe_name) catch |err| {
            std.log.err("Error creating run step info for {s}: {s}", .{ exe_name, @errorName(err) });
            return;
        };
        const run_step = b.step(run_step_info.name, run_step_info.description);

        const run_cmd = b.addRunArtifact(exe);

        run_step.dependOn(&run_cmd.step);
        run_cmd.step.dependOn(b.getInstallStep());

        if (b.args) |args| {
            run_cmd.addArgs(args);
        }
    }
}

fn createRunStepInfo(allocator: std.mem.Allocator, exe_name: []const u8) !RunStepInfo {
    const step_name = try std.fmt.allocPrint(allocator, "run:{s}", .{exe_name});
    const step_desc = try std.fmt.allocPrint(allocator, "Run {s} executable", .{exe_name});
    return .{ .name = step_name, .description = step_desc };
}

const RunStepInfo = struct {
    name: []const u8,
    description: []const u8,
};
