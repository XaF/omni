require 'singleton'
require 'yaml'

require_relative 'env'
require_relative 'utils'
require_relative 'config_command'


def stringify_keys(hash)
  return hash unless hash.is_a?(Hash)

  hash.map do |key, value|
    [key.to_s, stringify_keys(value)]
  end.to_h
end


class ConfigValue
  attr_reader :value, :path

  def initialize(value, path)
    @value = value
    @path = path
  end

  def to_s
    to_value.to_s
  end

  def to_value
    if value.is_a?(Array)
      value.map do |item|
        if item.is_a?(ConfigValue)
          item.to_value
        else
          item
        end
      end
    elsif value.is_a?(Hash)
      value.map do |key, item|
        [key, item.is_a?(ConfigValue) ? item.to_value : item]
      end.to_h
    elsif value.is_a?(ConfigValue)
      value.to_value
    else
      value
    end
  end
end


class Config
  include Singleton

  def self.default_config
    stringify_keys({
      auto_up_on_clone: true,
      cache_file: "#{ENV['HOME']}/.cache/omni",
      config_commands_split_on_dash: true,
      config_commands_split_on_slash: true,
      enable_git_repo_commands: true,
      enable_makefile_commands: true,
      env: [],
      makefile_commands_split_on_dash: true,
      makefile_commands_split_on_slash: true,
      path_repo_updates_enabled: true,
      path_repo_updates_interval: 12 * 60 * 60, # 12 hours
      repo_path_format: "%{host}/%{org}/%{repo}",
    })
  end

  def self.config_files
    [
      "#{ENV['HOME']}/.omni",
      "#{ENV['HOME']}/.omni.yaml",
      "#{ENV['HOME']}/.config/omni",
      "#{ENV['HOME']}/.config/omni.yaml",
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

  attr_reader :loaded_files

  def method_missing(method, *args, **kwargs, &block)
    if args.empty? && kwargs.empty? && block.nil? && config.has_key?(method.to_s)
      config[method.to_s]
    else
      super
    end
  end

  def respond_to_missing?(method, include_private = false)
    config.has_key?(method.to_s) || super
  end

  def initialize
    @loaded_files = []
    @config = import_values(self.class.default_config)

    self.class.config_files.each do |config_file|
      import(config_file)
    end

    if self.enable_git_repo_commands && OmniEnv.in_git_repo?
      import("#{OmniEnv.git_repo_root}/.omni")
      import("#{OmniEnv.git_repo_root}/.omni.yaml")
      import("#{OmniEnv.git_repo_root}/.omni/config")
      import("#{OmniEnv.git_repo_root}/.omni/config.yaml")
    end
  end

  def import(yaml_file)
    return if yaml_file.nil? || !File.file?(yaml_file) || !File.readable?(yaml_file)

    yaml = YAML::load(File.open(yaml_file))

    unless yaml.nil?
      error("invalid configuration file: #{yaml_file}") unless yaml.is_a?(Hash)
      #@config = recursive_merge_hashes(@config, stringify_keys(config))
      @config = import_values(yaml, file_path: yaml_file)
    end

    @loaded_files << yaml_file
  rescue Psych::SyntaxError
    error("invalid configuration file: #{yaml_file.yellow}", print_only: true)
  end

  def commands
    @commands ||= (@config['commands']&.value || {}).map do |command, config|
      ConfigCommand.new(command, config.to_value, path: config.path)
    rescue ArgumentError => e
      error(e.message, print_only: true)
      nil
    end.compact
  end

  def config
    @config.map do |key, value|
      [key, value.to_value]
    end.to_h
  end

  def with_src
    @config
  end

  private

  def import_values(values, file_path: nil, config: nil)
    config = @config&.dup || {} if config.nil?

    values.each do |key, value|
      value = import_values(value, file_path: file_path, config: config[key.to_s] || {}) if value.is_a?(Hash)
      config[key.to_s] = ConfigValue.new(value, file_path)
    end

    config
  end

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
