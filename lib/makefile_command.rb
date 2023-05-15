require 'pathname'

require_relative 'command'
require_relative 'config'
require_relative 'env'


class MakefileCommand < OmniCommand
  attr_reader :lineno

  def initialize(target, makefile, help: nil, lineno: nil, category: nil)
    @target = target
    @path = makefile
    @lineno = lineno

    @cmd = target.dup
    @cmd = [target] unless @cmd.is_a?(Array)
    @cmd.map! { |t| t.split('/') }.flatten! if Config.makefile_commands['split_on_slash']
    @cmd.map! { |t| t.split('-') }.flatten! if Config.makefile_commands['split_on_dash']

    cat = ['Makefile']

    # To make it clean, we can add the relative path to the
    # category in case the Makefile is not in the current
    # directory, so we can clearly see which makefile it is for
    relpath = Pathname.new(makefile).
      relative_path_from(Pathname.new(Dir.pwd)).
      to_s
    cat << relpath if relpath != cat[0]

    cat << category unless category.nil?


    # Prepare the short help, if any help was provided in the
    # Makefile for the target, which we consider being a
    # comment starting by '##'
    help_short = help || ''
    help_long = help_short

    @file_details = {
      category: cat,
      help_short: help_short,
      help_long: help_long,
      autocompletion: false,
      config_fields: Set.new,
      usage: nil,
      arguments: [],
      options: [],
      src: "#{relpath}:#{lineno}",
    }
  end

  def exec_command(*argv)
    # Switch to the Makefile directory
    Dir.chdir(File.dirname(@path))

    # Execute the command
    Kernel.exec('make', '-f', @path, @target, *argv)
  end
end
