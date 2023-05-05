#!/usr/bin/env ruby
#
# category: General
# config: up
# help: Sets up or tear down a repository depending on its \e[3mup\e[0m configuration
# help:
# help: \e[1m\e[3mUsage\e[0m\e[1m: omni \e[36m[\e[0m\e[1mup\e[36m|\e[0m\e[1mdown\e[36m]\e[0m

require_relative '../lib/colorize'
require_relative '../lib/config'
require_relative '../lib/utils'


class Operation
  attr_reader :config, :index

  def initialize(config, index: nil)
    @config = config
    @index = index

    check_valid_operation!
  end

  def up
    raise NotImplementedError
  end

  def down
    raise NotImplementedError
  end

  private

  def check_valid_operation!
    nil
  end

  def check_params(required_params: nil, allowed_params: nil, check_against: nil)
    check_against = config if check_against.nil?
    required_params ||= []
    allowed_params ||= []
    allowed_params.push(*required_params)

    required_params.each do |key|
      config_error("missing #{key.yellow}") unless check_against[key]
    end

    check_against.each_key do |key|
      config_error("unknown key #{key.yellow}") unless allowed_params.include?(key)
    end
  end

  def config_error(message)
    error("invalid #{'up'.yellow} configuration for "\
          "#{self.class.name.yellow}#{" (idx: #{index.to_s.yellow})" if index}: "\
          "#{message}")
  end

  def run_error(command)
    error("issue while running #{command.yellow}", print_only: true)
  end
end

class BundlerOperation < Operation
  def up
    STDERR.puts "#{"# Installing Gemfile dependencies with bundler".light_blue}#{" (#{path})".light_black if path}"

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
    return if OmniEnv.git_repo_root == File.dirname(File.dirname(__FILE__))

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

class HomebrewOperation < Operation
  def up
    STDERR.puts "# Installing Homebrew dependencies".light_blue

    tap_exists = false

    config.each do |pkg|
      pkgname, data = pkg.first
      version = data['version'] if data

      if version
        # Add new tap to fetch the version if needed
        unless tap_exists
          command_line("brew tap | grep -q #{tap_name} || brew tap-new #{tap_name}") || \
            (run_error("brew tap-new #{tap_name}") && next)
          tap_exists = true
        end

        # Extract the requested version
        command_line('brew', 'extract', '--version', version, pkgname, tap_name) || \
          (run_error("brew extract #{pkgname}") && next)

        # Change the package name so we install the right version
        pkgname = "#{pkgname}@#{version}"
      end

      command_line('brew', 'install', pkgname) || run_error("brew install #{pkgname}")
    end
  end

  def down
    STDERR.puts "# Uninstalling Homebrew dependencies".light_blue

    config.each do |pkg|
      pkgname, data = pkg.first
      version = data['version'] if data
      pkgname = "#{pkgname}@#{version}" if version

      command_line('brew', 'uninstall', pkgname) || run_error("brew uninstall #{pkgname}")
    end
  end

  private

  def tap_name
    'omni/local'
  end

  def check_valid_operation!
    @config = [{ config => {} }] if config.is_a?(String)

    @config = config.map do |key, value|
      { key => value }
    end if config.is_a?(Hash)

    config_error("expecting array, got #{config}") unless config.is_a?(Array)

    @config.map! do |item|
      item = { item => {} } if item.is_a?(String)
      config_error("expecting hash, got #{item}") unless item.is_a?(Hash)

      key, value = item.first
      value = { 'version' => value } if value.is_a?(String)
      check_params(allowed_params: ['version'], check_against: value)

      { key => value }
    end
  end
end

class CustomOperation < Operation
  def up
    if met?
      STDERR.puts "# Skipping #{name || config_meet} (already met)".light_yellow
      return
    end

    STDERR.puts "# #{name}".light_blue if name
    meet || run_error(name || config_meet)
  end

  def down
    return unless unmeet_cmd

    unless met?
      STDERR.puts "# Skipping revert of #{name || config_meet} (not met)".light_yellow
      return
    end

    STDERR.puts "# Revert: #{name}".light_blue if name
    unmeet || run_error(name || config_unmeet)
  end

  private

  def name
    config['name']
  end

  def met?
    return false unless met_cmd
    system("#{met_cmd} >/dev/null 2>/dev/null")
  end

  def meet
    command_line(meet_cmd)
  end

  def unmeet
    command_line(unmeet_cmd)
  end

  def met_cmd
    config['met?']
  end

  def meet_cmd
    config['meet']
  end

  def unmeet_cmd
    config['unmeet']
  end

  def check_valid_operation!
    config_error("expecting hash, got #{config}") unless config.is_a?(Hash)

    check_params(required_params: ['meet'], allowed_params: ['met?', 'name', 'unmeet'])
  end
end


error('too many arguments') if ARGV.size > 0
error("missing #{'up'.yellow} configuration") unless Config.respond_to?(:up) && Config.up
error("can only be run from a git repository") unless OmniEnv.in_git_repo?
error("invalid #{'up'.yellow} configuration, it should be a list") unless Config.up.is_a?(Array)

# Prepare all the commands that will need to be run, and check that the configuration is valid
operations = Config.up.each_with_index.map do |operation, idx|
  operation = { operation => {} } if operation.is_a?(String)
  error("invalid #{'up'.yellow} configuration for operation #{idx.to_s.yellow}") \
    unless operation.is_a?(Hash) && operation.size == 1

  optype = operation.keys.first
  opconfig = operation[optype]

  cls = begin
    Object.const_get("#{optype.capitalize}Operation")
  rescue NameError
    error("invalid #{'up'.yellow} configuration for operation #{idx.to_s.yellow}: unknown operation #{optype.yellow}")
  end

  error("invalid #{'up'.yellow} configuration for operation #{idx.to_s.yellow}: invalid operation #{optype.yellow}") \
    unless cls < Operation

  cls.new(opconfig, index: idx)
end

# Run the commands from the git repository root
Dir.chdir(OmniEnv.git_repo_root) do
  if OmniEnv::OMNI_SUBCOMMAND == 'up'
    # Run the operations in the provided order
    operations.each(&:up)
  else
    # In case of being called as `down`, this will also
    # run the operations in reverse order in case there
    # are dependencies between them
    operations.reverse.each(&:down)
  end
end
