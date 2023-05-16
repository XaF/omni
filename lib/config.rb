require 'singleton'
require 'yaml'

require_relative 'env'
require_relative 'utils'
require_relative 'config_command'


class ConfigUtils
  STRATEGY_REGEX = /^(?<key>.*)__(?<strategy>toappend|toprepend|toreplace|ifnone)$/

  def self.value_is_a?(value, klass)
    return if value.nil?
    value.is_a?(klass) || (value.is_a?(ConfigValue) && value.value.is_a?(klass))
  end

  def self.key_strategy(key, keypath, strategy)
    return [key, strategy] if strategy == :ignore_inherit

    # suggest_config is there so that we can suggest config edits to the user,
    # we thus do not want to touch or edit any keys under this, as it needs to
    # be able to be passed to the smart_merge function
    return [key, :ignore_inherit] if keypath.empty? && key == 'suggest_config'

    # path is a special key that is used to append or prepend to the path,
    # we do not want to merge this key, but rather append or prepend to it
    return [key, key.to_sym] if keypath == ['path'] && ['append', 'prepend'].include?(key)

    # if the key does not contain any strategy specification, we just use the
    # default strategy
    return [key, :default] unless key =~ STRATEGY_REGEX

    # If the key contains a strategy specification, we use that strategy
    key, strategy = $~[:key], $~[:strategy]
    strategy = strategy == 'ifnone' ? :keep : strategy.gsub(/^to/, '').to_sym

    [key, strategy]
  end

  def self.smart_merge(current, added, strategy: :default, keypath: nil, transform: method(:transform_unwrap), key_strategy: nil)
    keypath ||= []

    if value_is_a?(current, Hash) && value_is_a?(added, Hash)
      merged = current.dup
      added.each do |key, value|
        key_strategy = nil
        key_strategy ||= key_strategy.call(key, keypath, strategy) if key_strategy.respond_to?(:call)
        key_strategy ||= self.key_strategy(key, keypath, strategy)
        key, local_strategy = key_strategy

        oldval = merged[key]
        newval = value

        merged[key] = smart_merge(
          oldval, newval,
          strategy: local_strategy,
          keypath: keypath + [key],
          transform: transform,
        )
      end
      merged
    elsif !current&.nil? && !current&.empty? && strategy == :keep
      current
    elsif value_is_a?(current, Array) && value_is_a?(added, Array)
      start_index = case strategy
      when :prepend
        0
      when :append
        current.size
      else
        0
      end

      new_added = added.each_with_index.map do |value, index|
        smart_merge(
          nil, value,
          strategy: strategy == :ignore_inherit ? strategy : :default,
          keypath: keypath + [start_index + index],
          transform: transform,
        )
      end

      case strategy
      when :prepend
        new_added + current
      when :append
        current + new_added
      else
        new_added
      end.uniq
    elsif value_is_a?(added, Hash)
      added.map do |key, value|
        key_strategy = nil
        key_strategy ||= key_strategy.call(key, keypath, strategy) if key_strategy.respond_to?(:call)
        key_strategy ||= self.key_strategy(key, keypath, strategy)
        key, local_strategy = key_strategy

        new_value = smart_merge(
          nil, value,
          strategy: local_strategy,
          keypath: keypath + [key],
          transform: transform,
        )

        [key, new_value]
      end.to_h
    elsif value_is_a?(added, Array)
      new_added = []
      added.each_with_index do |value, index|
        new_added << smart_merge(
          nil, value,
          strategy: strategy == :ignore_inherit ? strategy : :default,
          keypath: keypath + [index],
          transform: transform,
      )
      end
      new_added
    elsif transform && transform.respond_to?(:call)
      transform.call(added, keypath)
    else
      added
    end
  end

  def self.transform_path(value, path, unwrap: true)
    if path.size == 3 && path[0] == 'path' && ['append', 'prepend'].include?(path[1])
      abs_path = value.value
      abs_path = File.join(File.dirname(value.path), abs_path) unless abs_path.start_with?('/')
      return abs_path if unwrap

      value.set_value(abs_path)
      return value
    end
    return ConfigUtils.transform_unwrap(value, path) if unwrap
    value
  end

  def self.transform_unwrap(value, path)
    value.is_a?(ConfigValue) ? value.value : value
  end
end


class ConfigValue
  def self.unwrap(value)
    return value unless value.is_a?(ConfigValue)
    value.unwrap
  end

  def self.wrap(value, path, labels: nil, wrapped: false, inheritance: false, return_found: false)
    found = {
      paths: [],
      labels: [],
    }

    value, found = if value.is_a?(ConfigValue)
      found[:paths] << value.path
      found[:paths].uniq!

      found[:labels].concat(value.labels)
      found[:labels].uniq!

      value.add_labels(labels) if labels

      [value, found]
    elsif value.is_a?(Hash) || value.is_a?(Array)
      value = if value.is_a?(Hash)
        value.map do |key, item|
          new_wrapped, local_found = wrap(
            item, path,
            labels: labels,
            inheritance: inheritance,
            return_found: true,
          )
          found[:paths] = (found[:paths] + local_found[:paths]).uniq
          found[:labels] = (found[:labels] + local_found[:labels]).uniq

          [key, new_wrapped]
        end.to_h
      else
        value.map do |item|
          new_wrapped, local_found = wrap(
            item, path,
            labels: labels,
            inheritance: inheritance,
            return_found: true,
          )
          found[:paths] = (found[:paths] + local_found[:paths]).uniq
          found[:labels] = (found[:labels] + local_found[:labels]).uniq

          new_wrapped
        end
      end

      unless wrapped
        value_path = inheritance && found[:paths].any? ? found[:paths].last : path
        value = ConfigValue.new(value, value_path, labels: labels)
      end

      [value, found]
    elsif wrapped
      [value, found]
    else
      [ConfigValue.new(value, path, labels: labels), found]
    end

    if return_found
      [value, found]
    else
      value
    end
  end

  attr_reader :value, :path, :labels

  def initialize(value, path = nil, labels: nil)
    @value = self.class.wrap(value, path, labels: labels, wrapped: true)
    @path = path
    @labels = labels || []
  end

  def method_missing(method, *args, **kwargs, &block)
    return @value.send(method, *args, **kwargs, &block) if @value.respond_to?(method)
    super
  end

  def respond_to_missing?(method, include_private = false)
    @value.respond_to?(method, include_private) || super
  end

  def set_value(value)
    @value = self.class.wrap(value, path, inheritance: true, wrapped: true)
  end

  def []=(key, value)
    @value[key] = self.class.wrap(value, path, inheritance: true)
  end

  def has_label?(label)
    labels.include?(label)
  end

  def add_labels(labels)
    @labels.concat(labels).uniq!
  end

  def reject_label(label)
    if has_label?(label)
      nil
    elsif @value.is_a?(ConfigValue)
      reject_value = @value.reject_label(label)
      return nil unless reject_value
      ConfigValue.new(reject_value, path, labels: labels)
    elsif @value.is_a?(Hash)
      reject_value = @value.map do |key, item|
        left = item.reject_label(label)
        next if left.nil?
        [key, left]
      end.compact.to_h
      return nil if reject_value.empty?
      ConfigValue.new(reject_value, path, labels: labels)
    elsif @value.is_a?(Array)
      reject_value = @value.map do |item|
        left = item.reject_label(label)
        next if left.nil?
        left
      end.compact
      return nil if reject_value.empty?
      ConfigValue.new(reject_value, path, labels: labels)
    else
      self
    end
  end

  def select_label(label)
    if @value.is_a?(ConfigValue)
      select_value = @value.select_label(label)
      return nil unless select_value
      ConfigValue.new(select_value, path, labels: labels)
    elsif @value.is_a?(Hash)
      select_value = @value.map do |key, item|
        left = item.select_label(label)
        next if left.nil?
        [key, left]
      end.compact.to_h
      return nil if select_value.empty?
      ConfigValue.new(select_value, path, labels: labels)
    elsif @value.is_a?(Array)
      select_value = @value.map do |item|
        left = item.select_label(label)
        next if left.nil?
        left
      end.compact
      return nil if select_value.empty?
      ConfigValue.new(select_value, path, labels: labels)
    elsif !has_label?(label)
      nil
    else
      self
    end
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

  def deep_dup
    copy = self.dup
    copy.instance_variable_set(:@value, @value.respond_to?(:deep_dup) ? @value.deep_dup : @value.dup)
    copy.instance_variable_set(:@labels, @labels.dup)
    copy.instance_variable_set(:@path, @path.dup)
    copy
  end
end


class Config
  include Singleton

  def self.default_config
    stringify_keys({
      cache: {
        path: "#{ENV['HOME']}/.cache/omni",
      },
      commands: {},
      command_match_skip_prompt_if: {
        first_min: 0.80,
        second_max: 0.60,
      },
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
      org: [],
      path: {
        append: [],
        prepend: [],
      },
      path_repo_updates: {
        enabled: true,
        interval: 12 * 60 * 60, # 12 hours
        ref_type: 'branch', # branch or tag
        ref_match: nil, # regex or nil
        per_repo_config: {
          # 'git@github.com:XaF/omni.git' => {
            # enabled: true,
            # ref_type: 'branch',
            # ref_match: 'master',
          # },
        }
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
    return config[method.to_s] if args.empty? && kwargs.empty? && block.nil? && config.has_key?(method.to_s)
    return config.send(method, *args, **kwargs, &block) if config.respond_to?(method)
    super
  end

  def respond_to_missing?(method, include_private = false)
    config.has_key?(method.to_s) || config.respond_to?(method) || super
  end

  def initialize
    @loaded_files = []
    @config = import_values(self.class.default_config)

    self.class.config_files.each do |config_file|
      import(config_file)
    end

    if OmniEnv.in_git_repo?
      [
        "#{OmniEnv.git_repo_root}/.omni",
        "#{OmniEnv.git_repo_root}/.omni.yaml",
        "#{OmniEnv.git_repo_root}/.omni/config",
        "#{OmniEnv.git_repo_root}/.omni/config.yaml",
      ].each do |config_file|
        import(config_file, labels: ['git_repo'])
      end
    end
  end

  def import(yaml_file, labels: nil)
    return if yaml_file.nil? || !File.file?(yaml_file) || !File.readable?(yaml_file)

    yaml = YAML::load(File.open(yaml_file))

    unless yaml.nil?
      error("invalid configuration file: #{yaml_file}") unless yaml.is_a?(Hash)
      @config = import_values(yaml, file_path: yaml_file, labels: labels)
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
    config.dig('path')&.unwrap || {}
  end

  def omnipath(include_local: true)
    paths = []

    config_paths = if include_local
      @config.dig('path')
    else
      @config.dig('path').reject_label('git_repo')
    end&.unwrap || {}

    paths.push(*config_paths['prepend']) if config_paths['prepend']&.any?
    paths.push(*OmniEnv::OMNIPATH) if OmniEnv::OMNIPATH&.any?
    paths.push(*config_paths['append']) if config_paths['append']&.any?
    paths.uniq!

    paths
  end

  def path_from_repo
    @path_from_repo ||= begin
      return {} unless OmniEnv.in_git_repo?

      @config.dig('path').select_label('git_repo')&.unwrap || {}
    end
  end

  def suggested_from_repo(unwrap: true)
    @suggested_from_repo ||= begin
      return {} unless OmniEnv.in_git_repo?
      return {} unless @config['suggest_config']
      # return {} unless @config['suggest_config'].has_label?('git_repo')

      suggest_config = @config['suggest_config'].
        select_label('git_repo')
    end

    suggest_config = @suggested_from_repo.dup
    suggest_config = suggest_config.map { |key, value| [key, value.unwrap] } if unwrap
    stringify_keys(suggest_config.to_h)
  end

  def config
    @config.map do |key, value|
      [key, value.unwrap]
    end.to_h
  end

  def with_src
    @config.deep_dup
  end

  def user_config_file(operation = :readonly, &block)
    # We check the files in reverse order as files are loaded in reverse
    # order of importance.
    user_config_files = self.class.config_files.reverse

    # We try and find first a config file that already exists and that is
    # writable, so that we can put our new user configuration in it.
    config_file = user_config_files.find do |config_file|
      File.file?(config_file) && File.readable?(config_file) && File.writable?(config_file)
    end

    # If we can't find a config file that already exists and that is writable,
    # and if the operation is not :readwrite, we can simply return an empty
    # config file here.
    if config_file.nil? && operation != :readwrite
      yield Hash.new
      return Hash.new
    end

    # If we can't find a config file that already exists and that is writable,
    # we try and find a config file that doesn't exist yet, but that is
    # writable, so that we can create it and put our new user configuration in it.
    config_file = user_config_files.find do |config_file|
      Pathname.new(config_file).ascend do |path|
        break File.writable?(path) if File.exist?(path)
      end
    end if config_file.nil?

    # If we can't find a config file that already exists and that is writable,
    FileUtils.mkdir_p(File.dirname(config_file))
    File.open(config_file, File::RDWR|File::CREAT, 0644) do |file|
      # Put a shared lock on the file
      file.flock(File::LOCK_EX)

      # Load the current configuration
      config = begin
        YAML::load(file) || {}
      rescue Psych::SyntaxError
        {}
      end

      # Yield the current configuration so that the caller
      # can read / update it
      new_config = yield config

      if operation == :readwrite
        return if new_config.nil?

        # Write the new configuration to the file
        file.rewind
        file.write(new_config.to_yaml)
        file.flush
        file.truncate(file.pos)
      end

      # Return the new configuration
      new_config
    end
  end

  private

  def transform_path(value, path)
    ConfigUtils.transform_path(value, path, unwrap: false)
  end

  def import_values(values, file_path: nil, config: nil, labels: nil)
    config = @config&.dup || ConfigValue.new({}) if config.nil?

    new_values = ConfigValue.new(values, file_path, labels: labels)
    merged = ConfigUtils.smart_merge(
      config, new_values,
      transform: method(:transform_path),
    )

    merged
  end
end
