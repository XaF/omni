#!/usr/bin/env ruby
#
# category: Git commands
# autocompletion: true
# config: cd
# opt:repo: The name of the repo to change directory to; this
# opt:repo: can be in the format <org>/<repo>, or just <repo>,
# opt:repo: in which case the repo will be searched for in all
# opt:repo: the organizations, trying to use \033[3mOMNI_ORG\033[0m if it is
# opt:repo: set, and then trying all the other organizations
# opt:repo: alphabetically.
# help: Change directory to the git directory of the specified repository
# help:
# help: If no repository is specified, change to the git directory of the
# help: main org as specified by \033[3mOMNI_ORG\033[0m, if specified, or errors out if
# help: not specified.

require_relative '../lib/colorize'
require_relative '../lib/lookup/repo'
require_relative '../lib/utils'


def autocomplete(argv)
  comp_cword = (ENV['COMP_CWORD'] || '1').to_i - 1

  # Since cd only takes one argument, we can just exit if we
  # are trying to autocomplete after the first argument
  exit 0 if comp_cword != 0

  # Try to autocomplete the repository name
  LookupRepo.autocomplete(argv[0])

  exit 0
end


autocomplete(ARGV[1..-1]) if ARGV[0] == '--complete'
error('too many argument') if ARGV[1]


# Use the first argument as the repository name
repo = ARGV[0]

# Lookup the repository
dir = LookupRepo.lookup(repo)

if dir
  omni_cmd('cd', dir)
  exit 0
end

# If we got here, we did not find the repository anywhere
error("#{repo.yellow}: No such repository") if repo
error('no path found')
