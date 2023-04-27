require 'singleton'
require 'yaml'

require_relative 'env'


class Config
  include Singleton

  def self.default_config
    {
      omni_cache_file: "#{ENV['HOME']}/.cache/omni",
      omni_path_repo_updates_enabled: true,
      omni_path_repo_updates_interval: 24 * 60 * 60, # 1 day
      omni_repo_path_format: "%{host}/%{org}/%{repo}",
    }
  end

  def self.config_files
    [
      "#{ENV['HOME']}/.omni",
      "#{ENV['HOME']}/.config/omni",
      ENV['OMNI_CONFIG'],
    ].compact
  end

  def self.method_missing(method, *args, &block)
    if self.instance.respond_to?(method)
      self.instance.send(method, *args, &block)
    else
      super
    end
  end

  def self.respond_to_missing?(method, include_private = false)
    self.instance.respond_to?(method, include_private) || super
  end

  def initialize
    parse_config(self.class.default_config)

    self.class.config_files.each do |config_file|
      import(config_file)
    end
  end

  def import(yaml_file)
    return if yaml_file.nil? || !File.exist?(yaml_file)

    config = YAML::load(File.open(yaml_file))
    parse_config(config)
  end

  private

  def parse_config(config)
    config.each do |key, value|
      setter = "#{key}="

      self.class.send(:attr_accessor, key) if !respond_to?(setter)
      send(setter, value)
    end
  end
end
