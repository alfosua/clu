vim.api.nvim_create_user_command('Zmake', function(opts)
  local target_dir = '.'
  if opts.args ~= '' then
    target_dir = opts.args
  end
  local build_file = vim.fs.joinpath(target_dir, "build.zig")
  local make_cmd = string.format('make --build-file %s', build_file)
  vim.cmd(make_cmd)
end, {
  desc = 'CLU: Build ABI library',
  nargs = '?',
  complete = 'dir',
})
