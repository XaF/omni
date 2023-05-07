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

  def arguments
    file_details[:arguments]
  end

  def optionals
    file_details[:optionals]
  end

  def src
    file_details[:src] ||= begin
      relpath = Pathname.new(path).
        relative_path_from(Pathname.new(Dir.pwd)).
        to_s

      if relpath.length < path.length
        relpath
      else
        path
      end
    end
  end

  def usage
    @usage ||= begin
      # Prepare the usage
      usage = "omni #{cmd.join(' ')}"

      if file_details[:usage]
        usage << " #{file_details[:usage]}"
      else
        if arguments.any?
          arguments.each do |arg|
            arg, desc = arg.first
            usage << " #{"<#{arg}>".light_cyan}"
          end
        end

        if optionals.any?
          optionals.each do |opt|
            opt, desc = opt.first
            usage << " #{"[#{opt}]".light_cyan}"
          end
        end
      end

      # Return the usage
      usage
    end
  end

  def exec(*argv, shift_argv: true)
    # Shift the argv if needed
    if shift_argv
      argv = argv.dup
      argv.shift(cmd.length)
    end

    # Prepare the environment variables
    Config.env.each { |key, value| ENV[key] = value.to_s }
    OmniEnv::set_env_vars
    ENV['OMNI_RUN_FROM'] = Dir.pwd
    ENV['OMNI_SUBCOMMAND'] = cmd.join(' ')

    # Execute the command
    Kernel.exec(path, *argv)

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
      argv.shift(cmd.length)
    end

    # Prepare the environment variables
    ENV['OMNI_SUBCOMMAND'] = cmd.join(' ')

    # Execute the command
    Kernel.exec(path, '--complete', *argv)

    # If we get here, the command failed
    exit 1
  end

  def config_fields
    file_details[:config_fields].to_a
  end

  def length
    cmd.join(' ').length
  end

  def start_with?(cmd_arr)
    cmd[0...cmd_arr.length] == cmd_arr || (
      cmd[0...cmd_arr.length - 1] == cmd_arr[0...cmd_arr.length - 1] &&
      cmd[cmd_arr.length - 1].start_with?(cmd_arr.last)
    )
  end

  def serves?(cmd_arr)
    cmd.length <= cmd_arr.length && cmd_arr[0...cmd.length] == cmd
  end

  def to_s_with_path
    "'#{cmd.join(' ')}' (#{path})"
  end

  def to_s
    cmd.join(' ')
  end

  def <=>(other)
    sort_key <=> other.sort_key
  end

  def sort_category
    if category.nil? || category.empty?
      # Downcase is always sorted after upcase, so by
      # using downcase for uncategorized, we make sure
      # that they will always end-up at the end!
      ['uncategorized']
    else
      category.map(&:upcase)
    end
  end

  def sort_command
    cmd.map(&:downcase)
  end

  def sort_key
    # We want to sort by category first, then by command
    [sort_category, sort_command]
  end

  private

  def file_details
    @file_details ||= begin
      # Prepare variables
      autocompletion = false
      category = nil
      config_fields = []
      help_lines = []
      params = {arg: {}, opt: {}}
      params_reg = /^# (?<type>arg|opt):(?<name>[^:]+):(?<desc>.*)$/
      usage = nil

      # Read the first few lines of the file, looking for lines starting with '# help:'
      File.open(path, 'r') do |file|
        reading_help = false
        file.each_line do |line|
          # Stop reading if the line does not start with '#' or if we started
          # reading the help and the line does not start with '# help:'
          break if line !~ /^#/ || (reading_help && line !~ /^# help:/)

          # Set the category if the line '# category: <category>' is found
          if line =~ /^# category:/
            category = line.sub(/^# category:\s?/, '').chomp
            next
          end

          # Set autocompletion to true if the line '# autocompletion: true' is found
          if line =~ /^#\s+autocompletion:/
            autocompletion = true if line =~ /^#\s+autocompletion:\s+true$/
            next
          end

          # Set the config fields if the line '# config: <field1>, <field2>, ...' is found
          if line =~ /^# config:/
            config_fields.concat(line.sub(/^# config:\s?/, '').chomp.split(',').map(&:strip))
            next
          end

          # Handle the color codes to make the colors appear
          if String.disable_colorization
            line = line.gsub(OmniCommand::COLOR_PATTERN, '')
          else
            line = line.gsub(OmniCommand::COLOR_PATTERN) { |m| eval("\"#{m}\"") }
          end

          # Parse arguments and options
          if line =~ /^# (arg|opt):/
            param = params_reg.match(line)
            next unless param

            type, name, desc = param[:type].to_sym, param[:name], param[:desc].strip
            if params[type][name]
              params[type][name][:desc] += "\n#{desc}"
            else
              params[type][name] = {desc: desc, pos: params[type].length}
            end
            next
          end

          # In case the usage was passed directly
          if line =~ /^# usage:/
            usage = line.sub(/^# usage:\s?/, '').chomp.strip
            next
          end

          # Check if we are reading the help
          reading_help = true if line =~ /^# help:/
          next unless reading_help

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
        config_fields: Set.new(config_fields),
        arguments: params[:arg].to_a.
          sort_by { |k, v| v[:pos] }.
          map { |k, v| [k, v[:desc]] },
        optionals: params[:opt].to_a.
          sort_by { |k, v| v[:pos] }.
          map { |k, v| [k, v[:desc]] },
        usage: usage,
      }
    end
  end
end

