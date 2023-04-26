require_relative 'env'


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

