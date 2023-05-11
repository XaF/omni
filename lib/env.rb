require 'uri'

def split_path(path, split_by: ':')
  path.split(split_by).compact.map(&:strip).reject(&:empty?)
end

class OmniEnv
  OMNIDIR = ENV['OMNIDIR'] || File.expand_path(File.join(File.dirname(__FILE__), '..'))
  OMNIPATH = split_path(ENV['OMNIPATH'] || File.expand_path(File.join(File.dirname(__FILE__), '..', 'cmd')))
  OMNI_CMD_FILE = (ENV['OMNI_CMD_FILE'] || '').empty? ? nil : ENV['OMNI_CMD_FILE']
  OMNI_GIT = ENV['OMNI_GIT'] || "#{ENV['HOME']}/git"
  OMNI_ORG = split_path(ENV['OMNI_ORG'] || '', split_by: ',')
  OMNI_SKIP_UPDATE = ENV['OMNI_SKIP_UPDATE'] == 'true'
  OMNI_SUBCOMMAND = (ENV['OMNI_SUBCOMMAND'] || '').empty? ? nil : ENV['OMNI_SUBCOMMAND']
  OMNI_UUID = (ENV['OMNI_UUID'] || '').empty? ? nil : ENV['OMNI_UUID']

  def self.set_env_vars
    ENV['OMNIPATH'] = OMNIPATH.join(':')
    ENV['OMNI_CMD_FILE'] = OMNI_CMD_FILE || ''
    ENV['OMNI_GIT'] = OMNI_GIT
    ENV['OMNI_ORG'] = OMNI_ORG.join(',')
  end

  def self.git_repo_root
    @@git_repo_root ||= `git rev-parse --show-toplevel 2>/dev/null`.strip
  end

  def self.git_repo_origin
    @@git_repo_origin ||= `git remote get-url origin 2>&1`.strip
  end

  def self.in_git_repo?
    self.git_repo_root != ''
  end

  def self.env
    {
      OMNIPATH: OMNIPATH,
      OMNI_CMD_FILE: OMNI_CMD_FILE,
      OMNI_GIT: OMNI_GIT,
      OMNI_ORG: OMNI_ORG,
      OMNI_SUBCOMMAND: OMNI_SUBCOMMAND,
      OMNI_UUID: OMNI_UUID,
      in_git_repo?: in_git_repo?,
      git_repo_root: git_repo_root,
      git_repo_origin: git_repo_origin,
    }
  end
end
