require 'shellwords'
require 'singleton'
require 'uri'


def split_path(path, split_by: ':')
  path.split(split_by).compact.map(&:strip).reject(&:empty?)
end


class OmniEnv
  include Singleton

  def self.method_missing(method, *args, **kwargs, &block)
    return self.instance.send(method, *args, **kwargs, &block) if self.instance.respond_to?(method)
    super
  end

  def self.respond_to_missing?(method, include_private = false)
    self.instance.respond_to?(method, include_private) || super
  end

  def self.const_missing(name)
    return omni_git if name == :OMNI_GIT
    return git_repo_root if name == :GIT_REPO_ROOT
    return git_repo_origin if name == :GIT_REPO_ORIGIN
    return in_git_repo? if name == :IN_GIT_REPO
    raise NameError, "uninitialized constant #{self.name}::#{name}"
  end

  OMNIDIR = ENV['OMNIDIR'] || File.expand_path(File.join(File.dirname(__FILE__), '..'))
  OMNIDIR_LOCATED = ENV['OMNIDIR_LOCATED'] == 'true'
  OMNIPATH = split_path(ENV['OMNIPATH'] || File.expand_path(File.join(File.dirname(__FILE__), '..', 'cmd')))
  OMNI_CMD_FILE = (ENV['OMNI_CMD_FILE'] || '').empty? ? nil : ENV['OMNI_CMD_FILE']
  OMNI_ORG = split_path(ENV['OMNI_ORG'] || '', split_by: ',')
  OMNI_SKIP_UPDATE = ENV['OMNI_SKIP_UPDATE'] == 'true'
  OMNI_FORCE_UPDATE = ENV['OMNI_FORCE_UPDATE'] == 'true'
  OMNI_SUBCOMMAND = (ENV['OMNI_SUBCOMMAND'] || '').empty? ? nil : ENV['OMNI_SUBCOMMAND']
  OMNI_UUID = (ENV['OMNI_UUID'] || '').empty? ? nil : ENV['OMNI_UUID']

  def set_env_vars
    ENV['OMNIPATH'] = OMNIPATH.join(':')
    ENV['OMNI_CMD_FILE'] = OMNI_CMD_FILE || ''
    ENV['OMNI_GIT'] = OMNI_GIT
    ENV['OMNI_ORG'] = OMNI_ORG.join(',')
  end

  def git_repo_root
    @git_repo_root ||= `git rev-parse --show-toplevel 2>/dev/null`.strip
  end

  def git_repo_origin
    @git_repo_origin ||= `git remote get-url origin 2>/dev/null`.strip
  end

  def in_git_repo?
    git_repo_root != ''
  end

  def env
    {
      OMNIPATH: OMNIPATH,
      OMNI_CMD_FILE: OMNI_CMD_FILE,
      OMNI_GIT: OMNI_GIT,
      OMNI_ORG: OMNI_ORG,
      OMNI_SUBCOMMAND: OMNI_SUBCOMMAND,
      OMNI_UUID: OMNI_UUID,
      IN_GIT_REPO: IN_GIT_REPO,
      GIT_REPO_ROOT: GIT_REPO_ROOT,
      GIT_REPO_ORIGIN: GIT_REPO_ORIGIN,
    }
  end

  def omni_git
    # First check if OMNI_GIT is set, in which case we just return it
    return ENV['OMNI_GIT'] if ENV['OMNI_GIT'] && !ENV['OMNI_GIT'].empty?

    # Otherwise check if the ~/git directory exists
    home_git = File.expand_path('~/git')
    return home_git if File.directory?(home_git) && File.writable?(home_git)

    # Otherwise, check if the GOPATH is set, and try to use GOPATH/src
    go_path = ENV['GOPATH']
    if go_path && !go_path.empty?
      go_git = File.join(go_path, 'src')

      first_existing_path = Pathname.new(go_git).ascend do |path|
        break path.to_s if path.exist?
      end

      return go_git if File.writable?(first_existing_path)
    end

    # Raise an error if we cannot resolve an OMNI_GIT repository
    error('Unable to resolve OMNI_GIT worktree, please configure it in your environment', cmd: 'env')
  end

  def user_shell
    current_pid = Process.pid

    process = loop do
      parent_pid = `ps -p #{current_pid} -oppid=`.strip.to_i

      # Break if we reach the top-level process or an error occurs
      break unless parent_pid > 1

      comm = `ps -p #{parent_pid} -ocommand=`.strip

      # Break and return comm as soon as we find a process that is not omni being run
      break comm unless comm =~ /^([a-z]*sh) #{Regexp.escape(File.join(OMNIDIR, 'bin', 'omni'))}( |$)/

      current_pid = parent_pid
    end

    unless process.nil?
      # Keep only the first word
      process = Shellwords.split(process).first

      # Remove starting dash if any
      process.sub!(/^-/, '')
    end

    process
  end

  def user_login_shell
    ENV['SHELL']
  end
end
