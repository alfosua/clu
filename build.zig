const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    // -------------------------------------------------------------------------
    // Core interfaces
    // -------------------------------------------------------------------------

    const mod_core_tools = b.addModule("core_tools", .{
        .root_source_file = b.path("modules/core/tools/src/root.zig"),
        .target = target,
        .optimize = optimize,
    });

    const mod_core_providers = b.addModule("core_providers", .{
        .root_source_file = b.path("modules/core/providers/src/root.zig"),
        .target = target,
        .optimize = optimize,
    });

    const mod_core_storages = b.addModule("core_storages", .{
        .root_source_file = b.path("modules/core/storages/src/root.zig"),
        .target = target,
        .optimize = optimize,
    });

    const mod_core_extensions = b.addModule("core_extensions", .{
        .root_source_file = b.path("modules/core/extensions/src/root.zig"),
        .target = target,
        .optimize = optimize,
    });
    mod_core_extensions.addImport("core_tools", mod_core_tools);

    mod_core_storages.addImport("core_providers", mod_core_providers);

    // -------------------------------------------------------------------------
    // Core agent (depends on all interfaces)
    // -------------------------------------------------------------------------

    const mod_core_agent = b.addModule("core_agent", .{
        .root_source_file = b.path("modules/core/agent/src/root.zig"),
        .target = target,
        .optimize = optimize,
    });
    mod_core_agent.addImport("core_tools", mod_core_tools);
    mod_core_agent.addImport("core_providers", mod_core_providers);
    mod_core_agent.addImport("core_storages", mod_core_storages);
    mod_core_agent.addImport("core_extensions", mod_core_extensions);

    // -------------------------------------------------------------------------
    // Provider implementations
    // -------------------------------------------------------------------------

    const mod_provider_openai_compat = b.addModule("provider_openai_compat", .{
        .root_source_file = b.path("modules/providers/openai-compat/src/root.zig"),
        .target = target,
        .optimize = optimize,
    });
    mod_provider_openai_compat.addImport("core_providers", mod_core_providers);

    // -------------------------------------------------------------------------
    // Storage backends
    // -------------------------------------------------------------------------

    const mod_storage_memory = b.addModule("storage_memory", .{
        .root_source_file = b.path("modules/storage/memory/src/root.zig"),
        .target = target,
        .optimize = optimize,
    });
    mod_storage_memory.addImport("core_storages", mod_core_storages);

    const mod_storage_jsonl = b.addModule("storage_jsonl", .{
        .root_source_file = b.path("modules/storage/jsonl/src/root.zig"),
        .target = target,
        .optimize = optimize,
    });
    mod_storage_jsonl.addImport("core_storages", mod_core_storages);

    // -------------------------------------------------------------------------
    // Tool implementations
    // -------------------------------------------------------------------------

    const mod_tools_bash = b.addModule("tools_bash", .{
        .root_source_file = b.path("modules/tools/bash/src/root.zig"),
        .target = target,
        .optimize = optimize,
    });
    mod_tools_bash.addImport("core_tools", mod_core_tools);

    const mod_tools_pwsh = b.addModule("tools_pwsh", .{
        .root_source_file = b.path("modules/tools/pwsh/src/root.zig"),
        .target = target,
        .optimize = optimize,
    });
    mod_tools_pwsh.addImport("core_tools", mod_core_tools);

    const mod_tools_fs = b.addModule("tools_fs", .{
        .root_source_file = b.path("modules/tools/fs/src/root.zig"),
        .target = target,
        .optimize = optimize,
    });
    mod_tools_fs.addImport("core_tools", mod_core_tools);

    const mod_tools_node = b.addModule("tools_node", .{
        .root_source_file = b.path("modules/tools/node/src/root.zig"),
        .target = target,
        .optimize = optimize,
    });
    mod_tools_node.addImport("core_tools", mod_core_tools);

    // -------------------------------------------------------------------------
    // Communication surfaces
    // -------------------------------------------------------------------------

    const mod_comms_repl = b.addModule("comms_repl", .{
        .root_source_file = b.path("modules/comms/repl/src/root.zig"),
        .target = target,
        .optimize = optimize,
    });
    mod_comms_repl.addImport("core_agent", mod_core_agent);

    const mod_comms_rpc = b.addModule("comms_rpc", .{
        .root_source_file = b.path("modules/comms/rpc/src/root.zig"),
        .target = target,
        .optimize = optimize,
    });
    mod_comms_rpc.addImport("core_agent", mod_core_agent);

    // -------------------------------------------------------------------------
    // Top-level CLI binary
    // -------------------------------------------------------------------------

    const exe_mod = b.createModule(.{
        .root_source_file = b.path("modules/clu/src/main.zig"),
        .target = target,
        .optimize = optimize,
    });
    exe_mod.addImport("core_agent", mod_core_agent);
    exe_mod.addImport("provider_openai_compat", mod_provider_openai_compat);
    exe_mod.addImport("storage_memory", mod_storage_memory);
    exe_mod.addImport("storage_jsonl", mod_storage_jsonl);
    exe_mod.addImport("tools_bash", mod_tools_bash);
    exe_mod.addImport("tools_pwsh", mod_tools_pwsh);
    exe_mod.addImport("tools_fs", mod_tools_fs);
    exe_mod.addImport("tools_node", mod_tools_node);
    exe_mod.addImport("comms_repl", mod_comms_repl);
    exe_mod.addImport("comms_rpc", mod_comms_rpc);

    const exe = b.addExecutable(.{
        .name = "clu",
        .root_module = exe_mod,
    });
    b.installArtifact(exe);

    // -------------------------------------------------------------------------
    // Run step: `zig build run`
    // -------------------------------------------------------------------------

    const run_cmd = b.addRunArtifact(exe);
    run_cmd.step.dependOn(b.getInstallStep());
    if (b.args) |args| run_cmd.addArgs(args);
    const run_step = b.step("run", "Run clu");
    run_step.dependOn(&run_cmd.step);

    // -------------------------------------------------------------------------
    // Test step: `zig build test`
    // -------------------------------------------------------------------------

    const test_step = b.step("test", "Run all unit tests");
    const test_sources: []const struct { name: []const u8, path: []const u8 } = &.{
        .{ .name = "core_tools",             .path = "modules/core/tools/src/root.zig" },
        .{ .name = "core_providers",         .path = "modules/core/providers/src/root.zig" },
        .{ .name = "core_storages",          .path = "modules/core/storages/src/root.zig" },
        .{ .name = "core_extensions",        .path = "modules/core/extensions/src/root.zig" },
        .{ .name = "core_agent",             .path = "modules/core/agent/src/root.zig" },
        .{ .name = "provider_openai_compat", .path = "modules/providers/openai-compat/src/root.zig" },
        .{ .name = "storage_memory",         .path = "modules/storage/memory/src/root.zig" },
        .{ .name = "storage_jsonl",          .path = "modules/storage/jsonl/src/root.zig" },
        .{ .name = "tools_bash",             .path = "modules/tools/bash/src/root.zig" },
        .{ .name = "tools_pwsh",             .path = "modules/tools/pwsh/src/root.zig" },
        .{ .name = "tools_fs",               .path = "modules/tools/fs/src/root.zig" },
        .{ .name = "tools_node",             .path = "modules/tools/node/src/root.zig" },
        .{ .name = "comms_repl",             .path = "modules/comms/repl/src/root.zig" },
        .{ .name = "comms_rpc",              .path = "modules/comms/rpc/src/root.zig" },
    };
    inline for (test_sources) |s| {
        const unit_tests = b.addTest(.{
            .name = s.name,
            .root_module = b.createModule(.{
                .root_source_file = b.path(s.path),
                .target = target,
                .optimize = optimize,
            }),
        });
        test_step.dependOn(&b.addRunArtifact(unit_tests).step);
    }
}
