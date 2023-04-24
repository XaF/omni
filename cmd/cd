#!/usr/bin/env ruby
#
# category: Git commands
# help: Change directory to the git directory of the specified repository
# help:
# help: If no repository is specified, change to the git directory of the
# help: main org as specified by \e[3mOMNI_ORG\e[0m, if specified, or errors out if
# help: not specified.
# help:
# help: \033[1m\e[3mUsage\e[0m\033[1m: omni cd \e[36m[repo]\e[0m
# help:
# help:   \e[36mrepo\e[0m      The name of the repo to change directory to; this
# help:             can be in the format <org>>/<repo>, or just <repo>,
# help:             in which case the repo will be searched for in all
# help:             the organizations, trying to use \e[3mOMNI_ORG\e[0m if it is
# help:             set, and then trying all the other organizations
# help:             alphabetically.

require 'colorize'

require_relative '../lib/env'
require_relative '../lib/omniorg'
require_relative '../lib/utils'

error('too many argument') if ARGV[1]
repo = ARGV[0]

paths = []

OmniOrgs.each do |org|
  paths << org.path?(repo)
end

paths << OmniEnv::OMNI_GIT unless repo

paths.compact!
paths.uniq!

paths.each do |path|
  next unless File.directory?(path)

  omni_cmd(['cd', path])
  exit 0
end

error("#{repo.yellow}: No such repository") if repo
error('no path found')
