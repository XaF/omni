#!/usr/bin/env ruby
#
# Main omni command, which is used to run the other commands
# and provide autocompletion for them

require_relative 'lib/colorize'
require_relative 'lib/lookup/command'
require_relative 'lib/path'
require_relative 'lib/updater'
require_relative 'lib/utils'


# complete_omni_subcommand is a function that will be called
# when the --complete flag is passed to omni. It will provide
# autocompletion for the subcommands
def complete_omni_subcommand(argv)
  # Check if we have the COMP_CWORD variable, which means
  # that we can know where the matching needs to happen
  comp_cword = (ENV['COMP_CWORD'] || '0').to_i - 1

  # Try to autocomplete the command
  LookupCommand.autocomplete(comp_cword, argv)

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
    UserInteraction.did_you_mean?(
      OmniPath, argv.join(' '),
      skip_with_score: Config.command_match_skip_prompt_if,
    ).exec(*argv)
  rescue UserInteraction::StoppedByUserError
    exit 0
  rescue UserInteraction::NoMatchError
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
