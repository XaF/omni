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

require 'find'

require_relative '../lib/colorize'
require_relative '../lib/env'
require_relative '../lib/omniorg'
require_relative '../lib/utils'


error('too many argument') if ARGV[1]


def basic_naive_lookup(repo)
  paths = OmniOrgs.map { |org| org.path?(repo) }
  paths << OmniEnv::OMNI_GIT unless repo

  paths.compact!
  paths.uniq!

  paths.each do |path|
    next unless File.directory?(path)

    return path
  end

  nil
end


def file_system_lookup(repo)
  return unless repo

  split_repo = repo.split('/')

  base_paths = OmniOrgs.map(&:path?).uniq

  progress_bar = TTYProgressBar.new(
    "#{"omni:".light_cyan} #{"cd:".yellow} Searching for repository [:bar]",
    bar_format: :triangle,
    clear: true,
    output: STDERR,
  )

  starting = Time.now
  begin
    potential_matches = []
    base_paths.each do |base_path|
      Find.find(base_path) do |path|
        Find.prune if base_paths.include?(path) and path != base_path

        next unless File.directory?(path)
        next unless File.basename(path) == '.git'

        progress_bar.advance

        dir_path = File.dirname(path)
        expected_match = dir_path.split('/')[-split_repo.length..-1]

        if expected_match == split_repo
          # Show a tip if the search took more than a second
          STDERR.puts "#{"omni:".light_cyan} #{"Did you know?".bold} A proper #{"OMNI_ORG".yellow} environment variable will make calls to #{'omni cd'.yellow} much faster." if Time.now - starting > 1
          return dir_path
        end

        potential_matches << dir_path
      end
    end
  ensure
    progress_bar.finish
    progress_bar.stop
  end

  # Exit if we did not find any potential matches
  return if potential_matches.empty?

  # If we got here and we did not find an exact match,
  # try offering a did-you-mean suggestion
  UserInterraction.did_you_mean?(potential_matches, repo)
end


# Use the first argument as the repository name
repo = ARGV[0]

# Try to find the repository by directly looking for
# it in our known paths
dir = basic_naive_lookup(repo)

# Try to find the repository by looking for it in
# the file system, under the main git directory,
# using find
begin
  dir = file_system_lookup(repo) unless dir
rescue UserInterraction::StoppedByUserError
  exit 0
rescue UserInterraction::NoMatchError
  nil
end

if dir
  omni_cmd(['cd', dir])
  exit 0
end

# If we got here, we did not find the repository anywhere
error("#{repo.yellow}: No such repository") if repo
error('no path found')
