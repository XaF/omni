#!/usr/bin/env ruby
#
# category: Git commands
# autocompletion: true
# arg:repo: The name of the repo to run commands in the context of; this
# arg:repo: can be in the format <org>/<repo>, or just <repo>,
# arg:repo: in which case the repo will be searched for in all
# arg:repo: the organizations, trying to use \033[3mOMNI_ORG\033[0m if it is
# arg:repo: set, and then trying all the other organizations
# arg:repo: alphabetically.
# arg:command: The omni command to run in the context of the specified
# arg:command: repository.
# opt:options...: Any options to pass to the omni command.
# help: Runs an omni command in the context of the specified repository
# help:
# help: This allows to run any omni command that would be available while
# help: in the repository directory, but without having to change directory
# help: to the repository first.

require_relative '../lib/colorize'
require_relative '../lib/lookup/repo'
require_relative '../lib/lookup/command'
require_relative '../lib/utils'


def autocomplete(argv)
  comp_cword = (ENV['COMP_CWORD'] || '1').to_i - 1

  # Since cd only takes one argument, we can just exit if we
  # are trying to autocomplete after the first argument
  if comp_cword == 0
    LookupRepo.autocomplete(argv[0])
  elsif comp_cword > 0
    # Call the autocomplete function for the command directly
    # by calling the command in the context of the repository
    dir = LookupRepo.lookup(argv[0])
    exit 0 unless dir

    Dir.chdir(dir) do
      # Shift the arguments to remove the repository name
      argv.shift

      # Prepare the environment for the autocompletion, we
      # do not want to decrease the value of comp_cword here
      # since we're already decreasing it in omni autocompletion
      env = {
        'COMP_CWORD' => comp_cword.to_s,
      }

      # Run the autocomplete command
      exec(env, 'omni', '--complete', *argv)

      # If we got here, the command failed
      exit 1
    end
  end

  exit 0
end


autocomplete(ARGV[1..-1]) if ARGV[0] == '--complete'
error('not enough arguments') unless ARGV[1]


# Use the first argument as the repository name
repo = ARGV[0]
error('no scope specified') unless repo

# Lookup the repository
dir = LookupRepo.lookup(repo)
error("#{repo.yellow}: No such repository") unless dir

# Now we just run the omni command from that repository
Dir.chdir(dir) do
  # Shift the arguments to remove the repository name
  ARGV.shift

  # Run the omni command
  system('omni', *ARGV)
end
