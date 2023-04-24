require 'find'

require_relative 'env'


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

  def self.sorted(&block)
    self.instance.sorted(&block)
  end

  def self.max_command_length
    self.instance.max_command_length
  end

  def each(&block)
    @each.each { |command| yield command } if block_given? && @each

    @each ||= begin
      each_commands = []

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
    @sorted ||= begin
      commands = each.to_a

      # Order the commands by category and then by command
      commands.sort_by! do |command|
        sorting_cmd = command.cmd.map(&:downcase)
        sorting_cat = if command.category.nil?
          # Downcase is always sorted after upcase, so by
          # using downcase for uncategorized, we make sure
          # that they will always end-up at the end!
          'uncategorized'
        else
          command.category.upcase
        end

        [sorting_cat, sorting_cmd]
      end

      commands
    end

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

  def initialize(cmd, path)
    @cmd = cmd
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
    @cmd[0...cmd_arr.length] == cmd_arr
  end

  def serves?(cmd_arr)
    @cmd.length <= cmd_arr.length && cmd_arr[0...@cmd.length] == @cmd
  end

  def to_s
    "'#{@cmd.join(' ')}' (#{@path})"
  end

  def cmd_s
    @cmd.join(' ')
  end

  private

  def file_details
    @file_details ||= begin
      # Prepare variables
      category = nil
      autocompletion = false

      # Read the first few lines of the file, looking for lines starting with '# help:'
      category = nil
      help_lines = []
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
          line = line.gsub(OmniCommand::COLOR_PATTERN) { |m| eval("\"#{m}\"") }

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
        category: category,
        help_short: help_short,
        help_long: help_long,
        autocompletion: autocompletion,
      }
    end
  end
end
