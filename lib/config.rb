require 'singleton'
require 'yaml'

require_relative 'env'


def stringify_keys(hash)
  return hash unless hash.is_a?(Hash)

  hash.map do |key, value|
    [key.to_s, stringify_keys(value)]
  end.to_h
end


class Config
  include Singleton

  def self.default_config
    stringify_keys({
      cache_file: "#{ENV['HOME']}/.cache/omni",
      path_repo_updates_enabled: true,
      path_repo_updates_interval: 12 * 60 * 60, # 12 hours
      repo_path_format: "%{host}/%{org}/%{repo}",
    })
  end

  def self.config_files
    [
      "#{ENV['HOME']}/.omni",
      "#{ENV['HOME']}/.config/omni",
      ENV['OMNI_CONFIG'],
    ].compact
  end

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

  attr_reader :loaded_files, :config

  def method_missing(method, *args, **kwargs, &block)
    if args.empty? && kwargs.empty? && block.nil? && @config.has_key?(method.to_s)
      @config[method.to_s]
    else
      super
    end
  end

  def respond_to_missing?(method, include_private = false)
    @config.has_key?(method.to_s) || super
  end

  def initialize
    @loaded_files = []
    @config = stringify_keys(self.class.default_config)

    self.class.config_files.each do |config_file|
      import(config_file)
    end
  end

  def import(yaml_file)
    return if yaml_file.nil? || !File.exist?(yaml_file)

    config = YAML::load(File.open(yaml_file))

    unless config.nil?
      error("invalid configuration file: #{yaml_file}") unless config.is_a?(Hash)
      @config = recursive_merge_hashes(@config, stringify_keys(config))
    end

    @loaded_files << yaml_file
  end

  private

  def recursive_merge_hashes(current_hash, added_hash)
    current_hash.merge(added_hash) do |key, current_val, added_val|
      if current_val.is_a?(Hash) && added_val.is_a?(Hash)
        recursive_merge_hashes(current_val, added_val)
      else
        added_val
      end
    end
  end
end
