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
    mod_core_providers.addImport("core_tools", mod_core_tools);

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

    // -------------------------------------------------------------------------
    // Compile-time configuration (baked into the binary)
    // Override at build time: zig build -Dbase_url=... -Dapi_key=... -Dmodel=...
    // -------------------------------------------------------------------------

    const build_options = b.addOptions();
    build_options.addOption(
        []const u8,
        "base_url",
        b.option([]const u8, "base_url", "Default API base URL") orelse "https://api.openai.com",
    );
    build_options.addOption(
        []const u8,
        "api_key",
        b.option([]const u8, "api_key", "Default API key") orelse "",
    );
    build_options.addOption(
        []const u8,
        "model",
        b.option([]const u8, "model", "Default model identifier") orelse "gpt-4o",
    );

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
    exe_mod.addOptions("build_options", build_options);

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

    // Reuse the already-wired named modules for tests so that all @import
    // declarations resolve correctly — each module already has its dependencies
    // added via addImport above.
    const test_step = b.step("test", "Run all unit tests");
    const test_modules: []const struct { name: []const u8, mod: *std.Build.Module } = &.{
        .{ .name = "core_tools",             .mod = mod_core_tools },
        .{ .name = "core_providers",         .mod = mod_core_providers },
        .{ .name = "core_storages",          .mod = mod_core_storages },
        .{ .name = "core_extensions",        .mod = mod_core_extensions },
        .{ .name = "core_agent",             .mod = mod_core_agent },
        .{ .name = "provider_openai_compat", .mod = mod_provider_openai_compat },
        .{ .name = "storage_memory",         .mod = mod_storage_memory },
        .{ .name = "storage_jsonl",          .mod = mod_storage_jsonl },
        .{ .name = "tools_bash",             .mod = mod_tools_bash },
        .{ .name = "tools_pwsh",             .mod = mod_tools_pwsh },
        .{ .name = "tools_fs",               .mod = mod_tools_fs },
        .{ .name = "tools_node",             .mod = mod_tools_node },
        .{ .name = "comms_repl",             .mod = mod_comms_repl },
        .{ .name = "comms_rpc",              .mod = mod_comms_rpc },
    };
    for (test_modules) |s| {
        const unit_tests = b.addTest(.{ .name = s.name, .root_module = s.mod });
        test_step.dependOn(&b.addRunArtifact(unit_tests).step);
    }
}
