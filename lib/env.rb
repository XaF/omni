require 'uri'

def split_path(path, split_by: ':')
  path.split(split_by).compact.map(&:strip).reject(&:empty?)
end

class OmniEnv
  OMNIPATH = split_path(ENV['OMNIPATH'] || File.expand_path(File.join(File.dirname(__FILE__), '..', 'cmd')))
  OMNI_CMD_FILE = ENV['OMNI_CMD_FILE'] || nil
  OMNI_GIT = ENV['OMNI_GIT'] || "#{ENV['HOME']}/git"
  OMNI_ORG = split_path(ENV['OMNI_ORG'] || '', split_by: ',')
  OMNI_SUBCOMMAND = ENV['OMNI_SUBCOMMAND'] || nil

  def self.set_env_vars
    ENV['OMNIPATH'] = OMNIPATH.join(':')
    ENV['OMNI_CMD_FILE'] = OMNI_CMD_FILE || ''
    ENV['OMNI_GIT'] = OMNI_GIT
    ENV['OMNI_ORG'] = OMNI_ORG.join(',')
  end

  def self.env
    {
      OMNIPATH: OMNIPATH,
      OMNI_CMD_FILE: OMNI_CMD_FILE,
      OMNI_GIT: OMNI_GIT,
      OMNI_ORG: OMNI_ORG,
      OMNI_SUBCOMMAND: OMNI_SUBCOMMAND,
    }
  end
end
