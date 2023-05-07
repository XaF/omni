require 'pathname'

require_relative 'command'
require_relative 'config'
require_relative 'env'


class ConfigCommand < OmniCommand
  def initialize(target, config, path: nil)
    raise ArgumentError, 'config must be a Hash' unless config.is_a?(Hash)
    raise ArgumentError, 'config must not be empty' if config.empty?
    raise ArgumentError, 'config must contain \'run\'' unless config.has_key?('run')

    @target = target
    @path = path
    @config = config

    @cmd = target.dup
    @cmd = [target] unless @cmd.is_a?(Array)
    @cmd.map! { |t| t.split('/') }.flatten! if Config.config_commands_split_on_slash
    @cmd.map! { |t| t.split('-') }.flatten! if Config.config_commands_split_on_dash

    cat = ['Configuration']
    category = config['category']
    unless category.nil?
      category = [category] unless category.is_a?(Array)
      cat.concat(category)
    end

    # Most of the time commands should come from the current repository,
    # but they might be coming from a different place too. This computes
    # the relative path from the current directory to the config file
    # containing the command, and displays it in the help.
    relpath = Pathname.new(path).
      relative_path_from(Pathname.new(Dir.pwd)).
      to_s if path
    cat << relpath unless path.nil? || path.start_with?("#{OmniEnv.git_repo_root}/")

    help_long = config['desc'] || ''
    help_short = help_long.split("\n").take_while { |l| l !~ /^\s*$/ }.join("\n")

    usage = nil
    arguments = []
    optionals = []
    if config.has_key?('syntax')
      syntax = config['syntax']

      if syntax.is_a?(String)
        usage = syntax
      elsif syntax.is_a?(Hash)
        if syntax.has_key?('arguments') || syntax.has_key?('argument')
          arguments = syntax['arguments'] || syntax['argument'] || []
          arguments = [arguments] unless arguments.is_a?(Array)
          arguments.map! { |arg| arg.is_a?(Hash) ? arg.first : [arg, ""] }
        end

        if syntax.has_key?('optionals') || syntax.has_key?('optional')
          optionals = syntax['optionals'] || syntax['optional'] || []
          optionals = [optionals] unless optionals.is_a?(Array)
          optionals.map! { |arg| arg.is_a?(Hash) ? arg.first : [arg, ""] }
        end
      end
    end

    help_long << "\n\n"
    help_long << "Imported from ".light_black
    help_long << relpath || 'default configuration'
    help_long.strip!

    @file_details = {
      category: cat,
      help_short: help_short,
      help_long: help_long,
      autocompletion: false,
      config_fields: Set.new,
      usage: usage,
      arguments: arguments,
      optionals: optionals,
    }
  end

  def exec(*argv, shift_argv: true)
    # Shift the argv if needed
    if shift_argv
      argv = argv.dup
      argv.shift(@cmd.length)
    end

    # Prepare the environment variables
    Config.env.each { |key, value| ENV[key] = value.to_s }
    env.each { |key, value| ENV[key] = value.to_s }
    OmniEnv::set_env_vars
    ENV['OMNI_RUN_FROM'] = Dir.pwd
    ENV['OMNI_SUBCOMMAND'] = @cmd.join(' ')

    # Switch to the directory containing the config file
    Dir.chdir(File.dirname(@path)) unless @path.nil?

    # Execute the command
    Kernel.exec('bash', '-c', @config['run'], @path, *argv)

    # If we get here, the command failed
    exit 1
  end

  def sort_category
    main_cat = @file_details[:category].first.downcase
    left_cat = @file_details[:category][1..-1].map(&:upcase)
    [main_cat, *left_cat]
  end

  private

  def env
    @config['env'] || {}
  end
end
