require 'uri'

def split_path(path, split_by: ':')
  path.split(split_by).compact.map(&:strip).reject(&:empty?)
end

module OmniEnv
  OMNIPATH = split_path(ENV['OMNIPATH'] || File.expand_path(File.join(File.dirname(__FILE__), '..', 'cmd')))
  OMNI_CMD_FILE = ENV['OMNI_CMD_FILE'] || nil
  OMNI_GIT = ENV['OMNI_GIT'] || "#{ENV['HOME']}/git"
  OMNI_ORG = split_path(ENV['OMNI_ORG'] || '', split_by: ',')
  OMNI_SUBCOMMAND = ENV['OMNI_SUBCOMMAND'] || nil
end
