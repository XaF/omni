#!/usr/bin/env ruby
#
# category: General
# help: Show help for omni commands
# help:
# help: If no command is given, show a list of all available commands.
# help:
# help: \e[1m\e[3mUsage\e[0m\e[1m: omni help \e[36m[command]\e[0m
# help:
# help:   \e[36mcommand\e[0m      The command to get help for

require_relative '../lib/colorize'
require_relative '../lib/path_alias'


# If we don't have a tty, we want to disable colorization
String.disable_colorization = true unless STDERR.tty?

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

  STDERR.puts "#{"omni".bold} - omnipotent tool"
  STDERR.puts ""
  STDERR.puts command.help_long
  STDERR.puts ""

  exit 0
end

# find longest command
ljust = [OmniPathWithAliases.max_command_length + 2, 15].max

# Find current width of the TTY
tty_current_width = `tput cols`.to_i

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

  help_short = command.help_short.split("\n").join(' ')
  help_short = help_short.scan(/(?:\A|\G(?<=\s)).{1,#{help_short_width}}(?=\s|$)|\S+/)

  # It's nicer to have the comma in the default color
  # while the commands are colored, so we need to color
  # _before_ joining the commands together
  cmd_str = "#{command.cmds_s.map { |cmd| cmd.cyan }.join(', ')}"

  # Since we added colorization, we cannot simply use ljust
  # to justify the strings, we need to add the missing
  # characters to the string length, which we'll compute
  # from the string without colorization
  cmd_str << ' ' * (ljust - command.cmds_s.join(', ').length)

  STDERR.puts "  #{cmd_str} #{help_short.first}"
  STDERR.puts "  #{" " * ljust} #{help_short[1..-1].join("\n   " + " " * ljust)}" if help_short.length > 1
end

STDERR.puts ""
