#!/usr/bin/env ruby
#
# category: Git commands
# config: clone={"auto_up": true}
# arg:repo: The repository to clone; this can be in
# arg:repo: format <org>/<repo>, just <repo>, or the full URL.
# arg:repo: If the case where only the repo name is specified,
# arg:repo: \e[3mOMNI_ORG\e[0m will be used to search for the
# arg:repo: repository to clone.
# help: Clone the specified repository

require_relative '../lib/colorize'
require_relative '../lib/config'
require_relative '../lib/env'
require_relative '../lib/omniorg'
require_relative '../lib/utils'


error('no repository specified') unless ARGV[0]
error('too many argument') if ARGV[1]

repo = ARGV[0]

omniRepo = OmniRepo.new(repo)

locations = [omniRepo]
locations = OmniOrgs.select { |org| org.remote?(repo) } unless omniRepo.remote?

error("#{repo.yellow}: No such repository") if locations.empty?

locations.each do |location|
  # Compute the path that we will use for the repository
  full_path = location.path?(repo)
  error("#{repo.yellow}: repository already exists #{"(#{full_path})".light_black}") if File.directory?(full_path)

  # Compute the remote address of the repository
  remote = location.remote?(repo)

  # Check using git ls-remote if the repository exists
  git_ls_remote = command_line('git', 'ls-remote', remote, '>/dev/null', '2>&1', timeout: 5)
  next unless git_ls_remote

  # Execute git command line from ruby
  git_clone = command_line('git', 'clone', remote, full_path)
  error("#{repo.yellow}: git clone failed") unless git_clone

  # Execute omni up from the repository directory if auto-up is enabled
  Dir.chdir(full_path) do
    omni_up = command_line('omni', 'up', env: { 'OMNI_SKIP_UPDATE' => 'true' })
    error("#{repo.yellow}: omni up failed") unless omni_up
  end if Config.dig('clone', 'auto_up').nil? || Config.dig('clone', 'auto_up')

  # Request omni to change directory to the newly-cloned repository
  omni_cmd(['cd', full_path])

  exit 0
end

error("#{repo.yellow}: unable to resolve repository")
