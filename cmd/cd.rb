#!/usr/bin/env ruby
#
# category: Git commands
# autocompletion: true
# help: Change directory to the git directory of the specified repository
# help:
# help: If no repository is specified, change to the git directory of the
# help: main org as specified by \e[3mOMNI_ORG\e[0m, if specified, or errors out if
# help: not specified.
# help:
# help: \e[1m\e[3mUsage\e[0m\e[1m: omni cd \e[36m[repo]\e[0m
# help:
# help:   \e[36mrepo\e[0m      The name of the repo to change directory to; this
# help:             can be in the format <org>>/<repo>, or just <repo>,
# help:             in which case the repo will be searched for in all
# help:             the organizations, trying to use \e[3mOMNI_ORG\e[0m if it is
# help:             set, and then trying all the other organizations
# help:             alphabetically.

require 'pathname'

require_relative '../lib/colorize'
require_relative '../lib/env'
require_relative '../lib/omniorg'
require_relative '../lib/utils'


def autocomplete(argv)
  comp_cword = (ENV['COMP_CWORD'] || '1').to_i - 1

  # Since cd only takes one argument, we can just exit if we
  # are trying to autocomplete after the first argument
  exit 0 if comp_cword != 0

  # Put the argument in a variable
  repo = argv[0]

  # If the repo starts with '.' or '/', the completion should
  # be path completion and not repo completion
  if repo && repo.start_with?('.', '/', '~/') || repo == '-'
    (Dir.glob("#{repo}*/") + Dir.glob("#{repo}*/**/*/")).sort.each do |match|
      puts match
    end unless repo == '-'

    exit 0
  end

  # We can try and fetch all the repositories that could start
  # with the value provided so far
  split_repo = (repo || '').split('/', -1)
  split_repo << '' if split_repo.empty?

  potential_matches = []
  OmniOrgs.repos(dedup: false) do |dir, path, dir_path|
    split_path = path.split('/')
    cmp_part = split_path[-split_repo.length..-1] || []

    next unless cmp_part[0..-2] == split_repo[0..-2] && cmp_part[-1].start_with?(split_repo[-1])

    match = if repo
      cmp_part.join('/')
    elsif dir == OmniEnv::OMNI_GIT
      path
    else
      Pathname.new(dir_path).relative_path_from(Pathname.new(OmniEnv::OMNI_GIT)).to_s
    end

    potential_matches << [
      dir_path,
      match,
    ]
  end

  if potential_matches&.any?
    # Filter and order the potential matches
    potential_matches.uniq! { |dir_path, _| dir_path }
    potential_matches.map! { |_, path| path }
    potential_matches.uniq!
    potential_matches.sort!

    # Write the potential matches if we find any
    potential_matches
      .each { |path| puts path }
  end

  exit 0
end


autocomplete(ARGV[1..-1]) if ARGV[0] == '--complete'
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

  progress_bar = TTYProgressBar.new(
    "#{"omni:".light_cyan} #{"cd:".yellow} Searching for repository [:bar]",
    bar_format: :triangle,
    clear: true,
    output: STDERR,
  )

  starting = Time.now
  begin
    potential_matches = []
    OmniOrgs.repos(dedup: false) do |dir, path, dir_path|
      progress_bar.advance

      expected_match = dir_path.split('/')[-split_repo.length..-1]

      if expected_match == split_repo
        # Show a tip if the search took more than a second
        STDERR.puts "#{"omni:".light_cyan} #{"Did you know?".bold} A proper #{"OMNI_ORG".yellow} environment variable will make calls to #{'omni cd'.yellow} much faster." if Time.now - starting > 1
        return dir_path
      end

      potential_matches << dir_path
    end
  ensure
    progress_bar.finish
    progress_bar.stop
  end

  # Exit if we did not find any potential matches
  return if potential_matches.empty?

  # If we got here and we did not find an exact match,
  # try offering a did-you-mean suggestion
  UserInterraction.did_you_mean?(potential_matches.uniq, repo)
end


# Use the first argument as the repository name
repo = ARGV[0]

# If the parameter starts with `.` or `/`, we can
# assume it is a path, and we can just try to cd
# to it
dir = repo if repo && repo.start_with?('/', '.', '~/') || repo == '-'

# Try to find the repository by directly looking for
# it in our known paths
dir = basic_naive_lookup(repo) unless dir

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
