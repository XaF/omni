#!/usr/bin/env ruby
#
# Main omni command, which is used to run the other commands
# and provide autocompletion for them

require_relative 'lib/colorize'
require_relative 'lib/path'
require_relative 'lib/updater'
require_relative 'lib/utils'


# complete_omni_subcommand is a function that will be called
# when the --complete flag is passed to omni. It will provide
# autocompletion for the subcommands
def complete_omni_subcommand(argv)
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

  # If we have full subcommands in the argv, we only want to
  # show the subcommands that match what's already provided,
  # but since we can delegate autocomplete, if this is not a
  # direct match, we will try reducing the constraints one
  # argument at a time and see if we can still get to something
  skip_elems = 0
  match = nil

  until match&.any? || skip_elems > match_pos
    skip_elems += 1

    match = commands.select do |omniCmd|
      omniCmd.cmd[0..match_pos - skip_elems] == argv[0..match_pos - skip_elems]
    end if match_pos > 0
  end
  commands = match if match&.any?

  if skip_elems == 1
    # For the last value in argv, we need to use more of a
    # matching with the start of the command
    match_last_val = commands.select do |omniCmd|
      omniCmd.cmd[match_pos]&.start_with?(argv[match_pos])
    end if argv.length > match_pos
    commands = match_last_val if match_last_val&.any?
  end

  if commands.length == 1 && commands[0].cmd.length <= match_pos
    omniCmd = commands[0]

    # If we get there, let's try and delegate calling --complete
    # to the underlying function in case it provides more
    # autocompletion...

    # Open the file and check the headers to see if it supports
    # autocompletion
    if omniCmd.autocompletion?
      # Set the environment variables that we need to pass to the
      # subcommand
      ENV['COMP_CWORD'] = (comp_cword - omniCmd.cmd.length + 1).to_s

      # Call the subcommand with the --complete flag, we delegate
      # the answer to it
      omniCmd.autocomplete(*argv)
    end
  end

  # If skip_elems is greater than 1, it means that we had to
  # go backward in our matching, and since we got here, it means
  # that we either didn't delegate the autocompletion process
  # to a subcommand or that the subcommand returned without
  # providing any autocompletion; we can thus exit here
  exit 0 if skip_elems > 1

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


# run_omni_subcommand is a function that will be called
# to run the subcommand that was passed to omni
def run_omni_subcommand(argv)
  # Run updates if delay has passed
  Updater.update

  # If no command was passed, we want to run the help command
  # instead; if the command is --help, we also want to run
  # the help command
  argv = ['help'] if argv.length == 0
  argv[0] = 'help' if ['--help', '-h'].include?(argv[0])

  # Try to find the requested command
  omniCmd = OmniPath.find { |omniCmd| omniCmd.serves?(argv) }
  omniCmd.exec(*argv) unless omniCmd.nil?

  # If we got here, it means that we didn't find the command
  # in any of the directories, so print an error message
  # and return an error code
  STDERR.puts "#{"omni:".light_cyan} #{"command not found:".red} #{ARGV.join(' ')}"

  # Prompt the user with a list of similar commands
  begin
    UserInterraction.did_you_mean?(
      OmniPath, argv.join(' '),
      skip_with_score: Config.command_match_skip_prompt_if,
    ).exec(*argv)
  rescue UserInterraction::StoppedByUserError
    exit 0
  rescue UserInterraction::NoMatchError
    nil
  end

  # Return an error code since we didn't find the command
  # to execute
  exit 1
end

# If the --complete flag is passed, we want to provide
# autocompletion for the subcommands
if ARGV.length > 0 && ARGV[0] == '--complete'
  argv = ARGV.dup
  argv.shift(1)
  complete_omni_subcommand(argv)
end


# Otherwise, we want to run the subcommand
run_omni_subcommand(ARGV.dup)
