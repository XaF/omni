require 'shellwords'

require_relative 'cache'
require_relative 'config'
require_relative 'env'
require_relative 'utils'


class Updater
  OMNI_PATH_UPDATE_CACHE_KEY = 'omni-path-updates'

  def self.update
    return unless Config.omni_path_repo_updates_enabled
    return if Cache.get(OMNI_PATH_UPDATE_CACHE_KEY, false)

    # Add first Omni's directory to the paths to update
    update_paths = [File.dirname(File.dirname(__FILE__))]
    update_paths.concat(OmniEnv::OMNIPATH)

    # Check if the path is a git repository, and if it is, get its toplevel
    update_paths.map! do |path|
      next unless File.directory?(path)

      # Build the command to get the toplevel
      command = ['git', '-C', path, 'rev-parse', '--show-toplevel']

      # Escape the command for safe execution
      command_safe = command.shelljoin

      # Run the command and get the output
      toplevel = `#{command_safe} 2>/dev/null`.chomp

      # If we didn't find a toplevel, we're not in a git repository
      # so we can just ignore it
      next unless toplevel

      toplevel
    end

    update_paths.compact!
    update_paths.uniq!

    STDERR.puts "#{"omni".light_cyan}: updating #{"OMNIPATH".yellow} repositories" if update_paths.any?

    # Update the repositories
    update_paths.each do |path|
      git_pull = command_line('git', 'pull', chdir: path, context: path)
      error("#{path.yellow}: git pull failed", cmd: 'updater', print_only: true) unless git_pull
    end

    Cache.set(OMNI_PATH_UPDATE_CACHE_KEY, true, expires_in: Config.omni_path_repo_updates_interval)
  end
end

# Updater.update
