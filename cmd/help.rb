#!/usr/bin/env ruby
#
# category: General
# autocompletion: true
# opt:command: The command to get help for
# help: Show help for omni commands
# help:
# help: If no command is given, show a list of all available commands.

require_relative '../lib/colorize'
require_relative '../lib/path_alias'


# autocomplete is a function that will be called when the --complete
# flag is passed to omni. It will provide autocompletion for the
# subcommands
def autocomplete(argv)
  commands = OmniPath.each.to_a

  # Check if we have the COMP_CWORD variable, which means
  # that we can know where the matching needs to happen
  comp_cword = (ENV['COMP_CWORD'] || '0').to_i - 1

  # Prepare until which word we need to match
  match_pos = if comp_cword >= 0
    comp_cword
  else
    argv.length - 1
  end

  commands.select! do |omniCmd|
    omniCmd.cmd[0..match_pos - 1] == argv[0..match_pos - 1]
  end if match_pos > 0

  # For the last value in argv, we need to use more of a
  # matching with the start of the command
  commands.select! do |omniCmd|
    omniCmd.cmd[match_pos]&.start_with?(argv[match_pos])
  end if argv.length > match_pos

  # If we have no commands, we can exit
  exit 0 if commands.length == 0

  # Extract the values at the expected position
  commands.map! { |omniCmd| omniCmd.cmd[match_pos] }
  commands.compact!
  commands.uniq!
  commands.sort!

  # Print the commands, one per line
  commands.each do |cmd|
    puts cmd
  end

  exit 0
end


# If the --complete flag is passed, we need to provide
# autocompletion for the subcommands
autocomplete(ARGV[1..-1]) if ARGV[0] == '--complete'

# Find current width of the TTY
tty_current_width = `bash -c "[[ \\"$TERM\\" || \\"$COLUMNS\\" ]] && tput cols || echo 100"`.to_i

# Define the max width we want to use on the screen
max_width = [tty_current_width - 4, 80].min

# format size allows to split a string at a given size
def format_size(str, size, return_split: false)
  str_split = str.split("\n\n")
  str_split.map! do |block|
    block.gsub("\n", ' ').
      scan(/(?<=\s|\A)(?:(?:\e(?:\[(?:\d+)(?:;\d+)*m))*.){1,#{size}}(?=\s|\z)/).
      map(&:strip).
      join("\n")
  end

  return str_split.join("\n\n") unless return_split

  str_split.map! { |block| block.split("\n") + [''] }
  str_split.flatten!
  str_split.pop if str_split.last == ''
  str_split
end

# If a specific command was passed as argument, show help
# for that command
if ARGV.length > 0
  search = ARGV.dup
  command = nil
  while command.nil? && search.length > 0
    command = OmniPath.find { |c| c.cmd == search }
    search.pop
  end

  if command.nil?
    STDERR.puts "#{"omni:".light_cyan} #{"command not found:".red} #{ARGV.join(' ')}"
    exit 1
  end

  usage_prefix = if String.disable_colorization
    'Usage'
  else
    "\e[1m\e[3mUsage\e[0m\e[1m"
  end

  all_params = command.arguments + command.options
  params_ljust = [
    (all_params.map(&:first).map(&:length).max || 0) + 2,
    10,
  ].max

  STDERR.puts "#{"omni".bold} - omnipotent tool"
  STDERR.puts ""
  if command.help_long.length > 0
    STDERR.puts format_size(command.help_long, max_width)
    STDERR.puts ""
  end
  STDERR.puts "#{usage_prefix}: #{"#{command.usage}".bold}"
  STDERR.puts ""
  all_params.each do |param, desc|
    next unless desc != ''
    desc = format_size(desc, max_width - params_ljust - 4, return_split: true)
    STDERR.puts "  #{param.ljust(params_ljust).cyan} #{desc.first}"
    STDERR.puts "  #{" " * params_ljust} #{desc[1..-1].join("\n   " + " " * params_ljust)}" if desc.length > 1
    STDERR.puts ""
  end

  if command.src && command.src.length > 0
    STDERR.puts "#{"Source:".light_black} #{command.src.underline}"
  end

  exit 0
end

# find longest command
ljust = [OmniPathWithAliases.max_command_length + 2, 15].max

# Compute short help width
help_short_width = tty_current_width - ljust - 4

# print help
STDERR.puts "#{"omni".bold} - omnipotent tool"
STDERR.puts ""
STDERR.puts "#{"Usage".italic}: omni #{"<command>".cyan} [options] ARG..."

last_cat = -1
OmniPathWithAliases.each do |command|
  if command.category != last_cat
    STDERR.puts ""

    cat = 'Uncategorized'.bold
    if !command.category.nil? && command.category.length > 0
      cat_elems = command.category.dup
      last_elem = cat_elems.pop
      cat_elems.map! { |elem| elem.light_black.bold }
      cat_elems << last_elem.bold

      cat_elems.reverse!
      cat = cat_elems.join(' < ')
    end

    STDERR.puts cat
    last_cat = command.category
  end

  # It's nicer to have the comma in the default color
  # while the commands are colored, so we need to color
  # _before_ joining the commands together
  cmd_str = "#{command.cmds_s.map { |cmd| cmd.cyan }.join(', ')}"

  # Since we added colorization, we cannot simply use ljust
  # to justify the strings, we need to add the missing
  # characters to the string length, which we'll compute
  # from the string without colorization
  cmd_str << ' ' * (ljust - command.cmds_s.join(', ').length)

  help_short = format_size(command.help_short, help_short_width, return_split: true)
  STDERR.puts "  #{cmd_str} #{help_short.first}"
  STDERR.puts "  #{" " * ljust} #{help_short[1..-1].join("\n   " + " " * ljust)}" if help_short.length > 1
end

STDERR.puts ""
