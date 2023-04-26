require 'find'
require 'pathname'

require_relative 'env'


class OmniPathForMakefiles
  TARGET_REGEX=%r{^([a-zA-Z_0-9\-\/\/]+):(.*?##\s*(.*))?$}

  @@instance = nil

  def self.instance
    @@instance ||= OmniPathForMakefiles.new
  end

  def self.each(&block)
    self.instance.each(&block)
  end

  def each(&block)
    @each.each { |command| yield command } if block_given? && @each

    @each ||= begin
      each_commands = []

      locate_files.each do |makefile|
        extract_targets(makefile) do |target, extra|
          omniCmd =  OmniCommandForMakefile.new(
            target,
            makefile,
            **extra,
          )

          yield omniCmd if block_given?

          each_commands << omniCmd
        end
      end

      each_commands
    end

    @each
  end

  private

  def locate_files
    @makefiles ||= begin
      makefiles = []

      current_dir = Dir.pwd

      # If we are in a git repository, we stop searching when
      # reaching the top level
      top_level = `git rev-parse --show-toplevel 2>/dev/null`.chomp
      top_level = Dir.home if top_level.empty?

      begin
        while Dir.pwd != '/'
          Dir.entries(Dir.pwd).each do |filename|
            next unless filename =~ /^(GNU)?Makefile(\..*)?$/
            makefiles << File.join(Dir.pwd, filename)
          end

          break if Dir.pwd == top_level

          Dir.chdir('..')
        end
      ensure
        Dir.chdir(current_dir)
      end

      makefiles
    end
  end

  def extract_targets(filepath)
    File.open(filepath, 'r') do |file|
      category = nil

      file.each_line.with_index do |line, lineno|
        category = $1 if line =~ /^##@\s*(.*)$/
        next unless line =~ TARGET_REGEX

        target = $1
        help = $3

        yield target, {help: help, lineno: lineno, category: category}
      end
    end
  end
end


class OmniCommandCollection < Array
  def push(command)
    return if find { |cmd| cmd.cmd == command.cmd }
    super(command)
  end

  def <<(command)
    return if find { |cmd| cmd.cmd == command.cmd }
    super(command)
  end
end


class OmniPath
  include Enumerable

  @@instance = nil

  def self.instance
    @@instance ||= OmniPath.new
  end

  def self.each(&block)
    self.instance.each(&block)
  end

  def self.map(&block)
    self.instance.map(&block)
  end

  def self.find(&block)
    self.instance.find(&block)
  end

  def self.select(&block)
    self.instance.select(&block)
  end

  def self.sorted(&block)
    self.instance.sorted(&block)
  end

  def self.max_command_length
    self.instance.max_command_length
  end

  def each(&block)
    @each.each { |command| yield command } if block_given? && @each

    @each ||= begin
      # By using this data structure, we make sure that no two commands
      # can have the same command call name; the second one will be
      # ignored.
      each_commands = OmniCommandCollection.new

      OmniEnv::OMNIPATH.each do |dirpath|
        next unless File.directory?(dirpath)

        Find.find(dirpath) do |filepath|
          next unless File.executable?(filepath) && File.file?(filepath)

          # remove the path from the command as prefix
          cmd = filepath.sub(/^#{Regexp.escape(dirpath)}\//, '').split('/')

          # Create and yield the OmniCommand object
          omniCmd = OmniCommand.new(cmd, filepath)
          yield omniCmd if block_given?

          each_commands << omniCmd
        end
      end

      OmniPathForMakefiles.each do |omniCmd|
        yield omniCmd if block_given?
        each_commands << omniCmd
      end

      each_commands
    end

    @each
  end

  def map(&block)
    return unless block_given?

    commands = []
    each do |command|
      commands << yield(command)
    end
    commands
  end

  def sorted(&block)
    @sorted ||= each.to_a.sort

    @sorted.each { |command| yield command } if block_given?

    @sorted
  end

  def max_command_length
    @max_command_length ||= map(&:length).max
  end
end


class OmniCommand
  COLOR_PATTERN = /\\(e|033)(\[(\d+)(;\d+)*m)/

  attr_reader :cmd, :path

  def self.cleanup_command(cmd)
    # Duplicate the command to avoid modifying the original
    cmd = cmd.dup

    # Split the directories and the file from the command
    last = cmd.pop

    # Remove the .d extension from the directories if present
    cmd.map! do |part|
      part.gsub(/\.d$/, '')
    end

    # Remove the extension from the file if present
    last = File.basename(last, File.extname(last))

    # Add back the file to the command
    cmd << last

    # Return the cleaned up command
    cmd
  end

  def initialize(cmd, path)
    @cmd = OmniCommand.cleanup_command(cmd)
    @path = path
  end

  def help_short
    file_details[:help_short]
  end

  def help_long
    file_details[:help_long]
  end

  def category
    file_details[:category]
  end

  def exec(*argv, shift_argv: true)
    # Shift the argv if needed
    if shift_argv
      argv = argv.dup
      argv.shift(@cmd.length)
    end

    # Prepare the environment variables
    OmniEnv::set_env_vars
    ENV['OMNI_RUN_FROM'] = Dir.pwd
    ENV['OMNI_SUBCOMMAND'] = @cmd.join(' ')

    # Execute the command
    Kernel.exec(@path, *argv)

    # If we get here, the command failed
    exit 1
  end

  def autocompletion?
    file_details[:autocompletion]
  end

  def autocomplete(*argv, shift_argv: true)
    # Shift the argv if needed
    if shift_argv
      argv = argv.dup
      argv.shift(@cmd.length)
    end

    # Prepare the environment variables
    ENV['OMNI_SUBCOMMAND'] = @cmd.join(' ')

    # Execute the command
    Kernel.exec(@path, '--complete', *argv)

    # If we get here, the command failed
    exit 1
  end

  def length
    @cmd.join(' ').length
  end

  def start_with?(cmd_arr)
    @cmd[0...cmd_arr.length] == cmd_arr || (
      @cmd[0...cmd_arr.length - 1] == cmd_arr[0...cmd_arr.length - 1] &&
      @cmd[cmd_arr.length - 1].start_with?(cmd_arr.last)
    )
  end

  def serves?(cmd_arr)
    @cmd.length <= cmd_arr.length && cmd_arr[0...@cmd.length] == @cmd
  end

  def to_s_with_path
    "'#{@cmd.join(' ')}' (#{@path})"
  end

  def to_s
    @cmd.join(' ')
  end

  def <=>(other)
    sort_key <=> other.sort_key
  end

  protected

  def sort_key
    sorting_cmd = cmd.map(&:downcase)
    sorting_cat = if category.nil? || category.empty?
      # Downcase is always sorted after upcase, so by
      # using downcase for uncategorized, we make sure
      # that they will always end-up at the end!
      ['uncategorized']
    else
      category.map(&:upcase)
    end

    # We want to sort by category first, then by command
    [sorting_cat, sorting_cmd]
  end

  private

  def file_details
    @file_details ||= begin
      # Prepare variables
      category = nil
      autocompletion = false
      help_lines = []

      # Read the first few lines of the file, looking for lines starting with '# help:'
      File.open(@path, 'r') do |file|
        reading_help = false
        file.each_line do |line|
          # Stop reading if the line does not start with '#' or if we started
          # reading the help and the line does not start with '# help:'
          break if line !~ /^#/ || (reading_help && line !~ /^# help:/)

          # Set the category if the line '# category: <category>' is found
          category = line.sub(/^# category:\s?/, '').chomp if line =~ /^# category:/

          # Set autocompletion to true if the line '# autocompletion: true' is found
          autocompletion = true if line =~ /^#\s+autocompletion:\s+true$/

          # Check if we are reading the help
          reading_help = true if line =~ /^# help:/
          next unless reading_help

          # Handle the color codes to make the colors appear
          if String.disable_colorization
            line = line.gsub(OmniCommand::COLOR_PATTERN, '')
          else
            line = line.gsub(OmniCommand::COLOR_PATTERN) { |m| eval("\"#{m}\"") }
          end

          # Add the line to the help lines
          help_lines << line
        end
      end

      # help_short is the help until the first empty help line
      help_long = help_lines.map { |l| l.sub(/^# help:\s?/, '').chomp }
      help_short = help_long.take_while { |l| l !~ /^\s*$/ }.join("\n")
      help_long = help_long.join("\n")

      # Return the file details
      {
        category: split_path(category || '', split_by: ','),
        help_short: help_short,
        help_long: help_long,
        autocompletion: autocompletion,
      }
    end
  end
end


class OmniCommandForMakefile < OmniCommand
  def initialize(target, makefile, help: nil, lineno: nil, category: nil)
    @target = target
    @path = makefile

    @cmd = if target.include?('/')
      target.split('/')
    elsif target.include?('-')
      target.split('-')
    else
      [target]
    end

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

    help_long = "#{help_short}"
    help_long << "\n\n"
    help_long << "\e[1m\e[3mUsage\e[0m\e[1m: omni #{@cmd.join(' ')}\e[0m"
    help_long << "\n\n"
    help_long << "Imported from ".light_black
    help_long << relpath
    help_long << ":".light_black if lineno
    help_long << "#{lineno.to_s}" if lineno
    help_long.strip!

    @file_details = {
      category: cat,
      help_short: help_short,
      help_long: help_long,
      autocompletion: false,
    }
  end

  def exec(*argv, shift_argv: true)
    # Shift the argv if needed
    if shift_argv
      argv = argv.dup
      argv.shift(@cmd.length)
    end

    # Prepare the environment variables
    OmniEnv::set_env_vars
    ENV['OMNI_RUN_FROM'] = Dir.pwd
    ENV['OMNI_SUBCOMMAND'] = @cmd.join(' ')

    # Switch to the Makefile directory
    Dir.chdir(File.dirname(@path))

    # Execute the command
    Kernel.exec('make', '-f', @path, @target, *argv)

    # If we get here, the command failed
    exit 1
  end

end
