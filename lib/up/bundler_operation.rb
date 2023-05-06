require_relative '../colorize'
require_relative '../utils'
require_relative 'operation'


class BundlerOperation < Operation
  def up
    STDERR.puts "#{"# Install Gemfile dependencies with bundler".light_blue}#{" (#{path})".light_black if path}"

    if path
      bundle_config = ['bundle', 'config', 'set', '--local', 'path', path]
      command_line(*bundle_config) || run_error("bundle config")
    end

    bundle_install = ['bundle', 'install']
    bundle_install.push('--gemfile', gemfile) if gemfile
    command_line(*bundle_install) || run_error("bundle install")
  end

  def down
    return unless path && Dir.exist?(path)
    return if OmniEnv.git_repo_root == OmniEnv::OMNIDIR

    STDERR.puts "# Removing dependencies installed with bundler".light_blue
    STDERR.puts "$ rm -rf #{path}".light_black
    FileUtils.rm_rf(path)
  end

  private

  def gemfile
    config['gemfile']
  end

  def path
    path = config['path']
    path = 'vendor/bundle' if path.nil?
    path
  end

  def check_valid_operation!
    @config = { 'gemfile' => config } if config.is_a?(String)
    config_error("expecting hash, got #{config}") unless config.is_a?(Hash)

    check_params(allowed_params: ['gemfile', 'path'])
  end
end

