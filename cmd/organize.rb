#!/usr/bin/env ruby
#
# category: Git commands
# autocompletion: true
# opt:--yes:Do not ask for confirmation before organizing repositories
# help: Organize your git repositories using the configured format
# help:
# help: This will offer to organize your git repositories, moving them from
# help: their current path to the path they should be at if they had been
# help: cloned using \033[3momni clone\033[0m. This is useful if you have a bunch of
# help: repositories that you have cloned manually, and you want to start
# help: using \033[3momni\033[0m, or if you changed your mind on the repo path format
# help: you wish to use.

require_relative '../lib/colorize'
require_relative '../lib/omniorg'
require_relative '../lib/utils'


options = SubcommandOptions({:yes => false, :up => false}) do |opts, options|
  opts.on("-y", "--yes", "Do not ask for confirmation before organizing repositories") do |yes|
    options[:yes] = yes
  end

  opts.on("-u", "--up-all", "Update all repositories and run 'omni up'") do |up|
    options[:up] = up
  end
end

error('too many argument') if ARGV[0]


class GitRepo
  attr_reader :path

  def initialize(dir_path)
    @path = dir_path
  end

  def remote
    'origin'
  end

  def git_url
    @git_url ||= `git -C "#{path}" remote get-url #{remote} 2>&1`
  end

  def expected_path
    @expected_path ||= OmniRepo.new(git_url)&.path?
  end

  def expected_path?
    expected_path.nil? || expected_path == path
  end

  def organizable?
    expected_path? || (expected_path && (!File.directory?(expected_path) || Dir.empty?(expected_path)))
  end

  def move!
    return unless organizable?
    FileUtils.mkdir_p(File.dirname(expected_path))
    FileUtils.rmdir(expected_path) if File.directory?(expected_path) && Dir.empty?(expected_path)
    FileUtils.mv(path, expected_path)
  end

  def cleanup!
    dir = File.dirname(path)
    while Dir.empty?(dir)
      FileUtils.rmdir(dir)
      dir = File.dirname(dir)
    end
  end

  def to_s
    s = +"#{path.light_red} => "
    s << if organizable?
      expected_path.light_green
    else
      "#{expected_path&.light_yellow} ⚠️"
    end

    s
  end
end


def list_file_system_repos(skip_confirmation: false)
  progress_bar = TTYProgressBar.new(
    "#{"omni:".light_cyan} #{"organize:".yellow} Searching for repositories [:bar]",
    bar_format: :triangle,
    clear: true,
    output: STDERR,
  )

  starting = Time.now
  begin
    reorg = []

    OmniOrgs.all_repos do |dir, path, dir_path|
      progress_bar.advance

      git_repo = GitRepo.new(dir_path)
      reorg << git_repo unless git_repo.expected_path?
    end
  ensure
    progress_bar.finish
    progress_bar.stop
  end

  # Exit if we did not find any potential matches
  if reorg.empty?
    STDERR.puts "#{"omni:".light_cyan} #{"organize:".yellow} All repositories are already organized! 🎉"
    return
  end

  unless skip_confirmation
    unless STDOUT.tty?
      STDERR.puts "#{"omni:".light_cyan} #{"organize:".yellow} Found #{reorg.length.to_s.bold} repositories to organize:"
      STDERR.puts reorg
      STDERR.puts "#{"omni:".light_cyan} #{"organize:".yellow} use #{"--yes".light_blue} to organize repositories"
      return
    end

    reorg = UserInteraction.which_ones?(
      "Found #{reorg.length.to_s.bold} repositor#{reorg.length > 1 ? 'ies' : 'y'} to organize:",
      reorg,
      default: (1..reorg.size).to_a.reverse,
      per_page: 10,
    )
  end

  # We go over the repositories and try to move them in their position;
  # since some repositories might be depending on other repositories being
  # moved first, we try looping until we can't move any more repositories
  left = reorg.length
  while reorg.any?
    need_reorg = reorg.dup
    while need_reorg.any?
      git_repo = need_reorg.shift
      next unless git_repo.organizable?

      STDERR.puts "#{"omni:".light_cyan} #{"organize:".yellow} Moving #{git_repo}"

      git_repo.move!
      git_repo.cleanup!

      reorg.delete(git_repo)
    end

    if left == reorg.length
      reorg.each do |git_repo|
        STDERR.puts "#{"omni:".light_cyan} #{"organize:".yellow} Skipping #{git_repo}"
      end

      break
    end

    left = reorg.length
  end
end

begin
  list_file_system_repos(skip_confirmation: options[:yes])
rescue UserInteraction::StoppedByUserError, UserInteraction::NoMatchError
  nil
end

