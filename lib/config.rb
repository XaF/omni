require 'yaml'

require_relative 'env'


class Config
  def self.default_config
    {
      omni_cache_file: "#{ENV['HOME']}/.cache/omni",
      omni_path_repo_updates_enabled: true,
      omni_path_repo_updates_interval: 24 * 60 * 60, # 1 day
      omni_repo_path_format: "%{host}/%{org}/%{repo}",
    }
  end

  @@instance = nil

  def self.instance
    @@instance ||= begin
      instance = Config.new

      instance.import("#{ENV['HOME']}/.omni")
      instance.import("#{ENV['HOME']}/.config/omni")
      instance.import(ENV['OMNI_CONFIG']) if ENV['OMNI_CONFIG']

      instance
    end
  end

  def self.method_missing(method, *args, &block)
    if instance.respond_to?(method)
      instance.send(method, *args, &block)
    else
      super
    end
  end

  def self.respond_to_missing?(method, include_private = false)
    instance.respond_to?(method, include_private) || super
  end

  def initialize(yaml_file = nil)
    parse_config(self.class.default_config)

    import(yaml_file) if !yaml_file.nil?
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
