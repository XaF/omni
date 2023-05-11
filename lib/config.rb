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
  def self.unwrap(value)
    return value unless value.is_a?(ConfigValue)
    value.unwrap
  end

  def self.wrap(value, path, wrap_obj: true, wrapped: false)
    if value.is_a?(ConfigValue)
      value
    elsif value.is_a?(Hash)
      value = value.map do |key, item|
        [key, wrap(item, path, wrap_obj: true)]
      end.to_h
      wrapped ? value : ConfigValue.new(value, path)
    elsif value.is_a?(Array)
      value = value.map do |item|
        wrap(item, path, wrap_obj: true)
      end
      wrapped ? value : ConfigValue.new(value, path)
    elsif !wrap_obj
      value
    else
      ConfigValue.new(value, path)
    end
  end

  attr_reader :value, :path

  def initialize(value, path = nil)
    @value = self.class.wrap(value, path, wrap_obj: false, wrapped: true)
    @path = path
  end

  def method_missing(method, *args, **kwargs, &block)
    if @value.respond_to?(method)
      @value.send(method, *args, **kwargs, &block)
    else
      super
    end
  end

  def respond_to_missing?(method, include_private = false)
    @value.respond_to?(method, include_private) || super
  end

  def []=(key, value)
    @value[key] = self.class.wrap(value, path)
  end

  def to_s
    unwrap.to_s
  end

  def unwrap
    if value.is_a?(Array)
      value.map do |item|
        if item.is_a?(ConfigValue)
          item.unwrap
        else
          item
        end
      end
    elsif value.is_a?(Hash)
      value.map do |key, item|
        [key, item.is_a?(ConfigValue) ? item.unwrap : item]
      end.to_h
    elsif value.is_a?(ConfigValue)
      value.unwrap
    else
      value
    end
  end
end


class Config
  include Singleton

  def self.default_config
    stringify_keys({
      cache: {
        path: "#{ENV['HOME']}/.cache/omni",
      },
      clone: {
        auto_up: true,
      },
      commands: {},
      config_commands: {
        split_on_dash: true,
        split_on_slash: true,
      },
      env: {},
      makefile_commands: {
        enabled: true,
        split_on_dash: true,
        split_on_slash: true,
      },
      path: {
        append: [],
        prepend: [],
      },
      path_repo_updates: {
        enabled: true,
        interval: 12 * 60 * 60, # 12 hours
      },
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

    @path = {
      append: [],
      prepend: [],
    }

    self.class.config_files.each do |config_file|
      import(config_file)
    end

    if OmniEnv.in_git_repo?
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
      @config = import_values(yaml, file_path: yaml_file)
    end

    if yaml['path'] && yaml['path'].is_a?(Hash)
      if yaml['path']['append'] && yaml['path']['append'].is_a?(Array)
        yaml['path']['append'].each do |path|
          path = File.join(File.dirname(yaml_file), path) if path[0] != '/'
          @path[:append] << ConfigValue.new(path, yaml_file)
        end
      end
      if yaml['path']['prepend'] && yaml['path']['prepend'].is_a?(Array)
        yaml['path']['prepend'].each do |path|
          path = File.join(File.dirname(yaml_file), path) if path[0] != '/'
          @path[:prepend].unshift(ConfigValue.new(path, yaml_file))
        end
      end
    end

    @loaded_files << yaml_file
  rescue Psych::SyntaxError
    error("invalid configuration file: #{yaml_file.yellow}", print_only: true)
  end

  def commands
    @commands ||= (@config['commands']&.value || {}).map do |command, config|
      ConfigCommand.new(command, config.unwrap, path: config.path)
    rescue ArgumentError => e
      error(e.message, print_only: true)
      nil
    end.compact
  end

  def path
    stringify_keys(ConfigValue.new(@path, nil).unwrap)
  end

  def config
    @config.map do |key, value|
      [key, value.unwrap]
    end.to_h
  end

  def with_src
    config_copy = @config.dup
    config_copy['path'] = ConfigValue.new(stringify_keys(@path), nil)
    config_copy
  end

  private

  def import_values(values, file_path: nil, config: nil)
    config = @config&.dup || {} if config.nil?

    values.each do |key, value|
      if value.is_a?(Hash) && config[key.to_s].is_a?(Hash)
        value = import_values(value, file_path: file_path, config: config[key.to_s] || {})
      end
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
