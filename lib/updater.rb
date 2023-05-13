require 'shellwords'

require_relative 'cache'
require_relative 'config'
require_relative 'env'
require_relative 'omniorg'
require_relative 'utils'


class Updater
  include Singleton

  OMNI_PATH_UPDATE_CACHE_KEY = 'omni-path-updates'

  def self.method_missing(method, *args, **kwargs, &block)
    if self.instance.respond_to?(method)
      self.instance.send(method, *args, **kwargs, &block)
    else
      super
    end
  end

  def self.respond_to_missing?(method, include_private = false)
    self.instance.respond_to?(method, include_private) || super
  end

  def update
    return unless should_update?
    return unless update_paths.any?

    STDERR.puts "#{"omni".light_cyan}: updating #{"OMNIPATH".yellow} repositories"

    # Update the repositories
    update_paths.each do |path, repo_id|
      repo_config = update_config(repo_id)
      next unless repo_config['enabled']

      unless ['branch', 'tag'].include?(repo_config['ref_type'])
        error("#{repo_id.yellow}: invalid ref_type #{repo_config['ref_type'].inspect}", cmd: 'updater', print_only: true)
        next
      end

      Dir.chdir(path) do
        updated = if repo_config['ref_type'] == 'branch'
          update_using_branch(path, repo_config['ref_match'])
        elsif repo_config['ref_type'] == 'tag'
          update_using_tag(path, repo_config['ref_match'])
        else
          error("#{repo_id.yellow}: Urgh!? How did we get there? I must be a teapot or something.",
                cmd: 'updater', print_only: true)
          false
        end

        next unless updated

        omni_up = command_line('omni', 'up', context: path, env: { 'OMNI_SKIP_UPDATE' => 'true' })
        error("#{path.yellow}: omni up failed", cmd: 'updater', print_only: true) unless omni_up
      end
    end
  end

  private

  def update_using_branch(path, branch = nil)
    if branch
      # Check if the currently checked out branch matches the one we want to update
      git_branch_command = ['git', 'branch', '--show-current']
      git_branch = `#{git_branch_command.shelljoin} 2>/dev/null`.chomp

      unless git_branch && Regexp.new(branch).match?(git_branch)
        msg = if git_branch
          "current branch #{git_branch.inspect} does not match #{branch.inspect}"
        else
          "does not seem to have a branch checked out"
        end
        warning("#{path.yellow}: #{msg}; skipping", cmd: 'updater')
        return false
      end
    end

    git_pull = command_line('git', 'pull', '--ff-only', context: path, capture: true)
    unless git_pull[:return_code].zero?
      error("#{path.yellow}: git pull failed", cmd: 'updater', print_only: true)
      return false
    end

    if git_pull[:out].size == 1 && git_pull[:out][0][:line].include?('Already up to date.')
      # If repo is already up to date, nothing more to do!
      return false
    end

    true
  end

  def update_using_tag(path, tag = nil)
    # Check if we are currently checked out on a tag
    checked_tag_command = ['git', 'tag', '--points-at', 'HEAD', '--sort=-creatordate']
    checked_tag = `#{checked_tag_command.shelljoin} 2>/dev/null`.chomp
    unless checked_tag
      warning("#{path.yellow}: not currently checked out on a tag; skipping", cmd: 'updater')
      return false
    end

    # Consider the latest tag built on this commit to be the current tag
    current_tag = checked_tag.split("\n").first

    # Fetch all the tags for the repository
    git_fetch = command_line('git', 'fetch', '--tags', context: path, capture: true)
    unless git_fetch[:return_code].zero?
      error("#{path.yellow}: git fetch failed", cmd: 'updater', print_only: true)
      return false
    end

    # Check if there was any new tags fetched
    if git_fetch[:out].empty? && git_fetch[:err].empty?
      # If no new tags, nothing more to do!
      return false
    end

    # If any new tags, we need to check what is the most recent tag
    # that matches the passed tag parameter (if any)
    git_tag_command = ['git', 'tag', '--sort=-creatordate']
    git_tag = `#{git_tag_command.shelljoin} 2>/dev/null`.chomp
    unless git_tag
      warning("#{path.yellow}: no git tag found; skipping", cmd: 'updater')
      return false
    end

    # Find the most recent git tag in git_tags that matches
    # the passed tag parameter (if any)
    git_tags = git_tag.split("\n")
    tag_regex = tag ? Regexp.new(tag) : nil
    target_tag = git_tags.find do |git_tag|
      tag_regex ? tag_regex.match?(git_tag) : true
    end

    if target_tag.nil?
      warning("#{path.yellow}: no matching git tag found; skipping", cmd: 'updater')
      return false
    end

    if target_tag == current_tag
      # If repo is already up to date, nothing more to do!
      return false
    end

    # Checkout the target tag
    git_checkout = command_line('git', 'checkout', '--no-guess', target_tag, context: path)
    unless git_checkout
      error("#{path.yellow}: git checkout failed", cmd: 'updater', print_only: true)
      return false
    end

    true
  end

  def should_update?
    return false if OmniEnv::OMNI_SKIP_UPDATE
    return true if OmniEnv::OMNI_FORCE_UPDATE
    return false unless Config.path_repo_updates['enabled']

    # Check if we've updated recently
    Cache.exclusive(
      OMNI_PATH_UPDATE_CACHE_KEY,
      expires_in: Config.path_repo_updates['interval'],
    ) do |updated_recently|
      return false if updated_recently&.value
      true
    end

    true
  end

  def update_paths
    @update_paths ||= begin
      # Add first Omni's directory to the paths to update
      update_paths = [File.dirname(File.dirname(__FILE__))]
      update_paths.concat(Config.omnipath(include_local: false))

      # Check if the path is a git repository, and if it is, get its toplevel
      update_paths.map! do |path|
        next unless File.directory?(path)

        # Figure out the toplevel
        toplevel_command = ['git', '-C', path, 'rev-parse', '--show-toplevel']
        toplevel = `#{toplevel_command.shelljoin} 2>/dev/null`.chomp

        # If we didn't find a toplevel, we're not in a git repository
        # so we can just ignore it
        next unless toplevel

        toplevel
      end

      update_paths.compact!
      update_paths.uniq!

      # Now that we have unique paths, we can grab their remote to
      # match the repository to its expected configuration
      update_paths.map! do |path|
        # Figure out the remote URL
        remote_command = ['git', '-C', path, 'remote', 'get-url', 'origin']
        remote = `#{remote_command.shelljoin} 2>/dev/null`.chomp

        next unless remote

        id = begin
          OmniRepo.new(remote).id
        rescue ArgumentError
          warning("#{path.yellow}: #{remote.inspect} is not a valid remote; skipping", cmd: 'updater')
          next
        end

        [path, id]
      end

      update_paths.compact!

      update_paths
    end
  end

  def update_config(repo = nil)
    @default_config ||= begin
      stringify_keys({
        enabled: true,
        ref_type: 'branch',
        ref_name: nil,
      }).merge(
        (Config.dig('path_repo_updates') || {}).
          slice('enabled', 'ref_type', 'ref_name')
      )
    end

    return @default_config unless repo
    return @default_config unless Config.path_repo_updates['per_repo_config']
    return @default_config unless Config.path_repo_updates['per_repo_config'][repo]

    @repo_config ||= {}
    @repo_config[repo] ||= @default_config.merge(Config.path_repo_updates['per_repo_config'][repo])
    @repo_config[repo]
  end
end
