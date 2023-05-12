require 'singleton'
require 'yaml'

require_relative 'env'
require_relative 'utils'
require_relative 'config_command'


class ConfigValue
  def self.unwrap(value)
    return value unless value.is_a?(ConfigValue)
    value.unwrap
  end

  def self.wrap(value, path, labels: nil, wrap_obj: true, wrapped: false)
    if value.is_a?(ConfigValue)
      value.add_labels(labels) if labels
      value
    elsif value.is_a?(Hash)
      value = value.map do |key, item|
        [key, wrap(item, path, labels: labels, wrap_obj: true)]
      end.to_h
      wrapped ? value : ConfigValue.new(value, path)
    elsif value.is_a?(Array)
      value = value.map do |item|
        wrap(item, path, labels: labels, wrap_obj: true)
      end
      wrapped ? value : ConfigValue.new(value, path, labels: labels)
    elsif !wrap_obj
      value
    else
      ConfigValue.new(value, path, labels: labels)
    end
  end

  attr_reader :value, :path, :labels

  def initialize(value, path = nil, labels: nil)
    @value = self.class.wrap(value, path, wrap_obj: false, wrapped: true)
    @path = path
    @labels = labels || []
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

  def has_label?(label)
    labels.include?(label)
  end

  def add_labels(labels)
    @labels += labels
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

    @path = {
      append: [],
      prepend: [],
    }

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

    if yaml['path'] && yaml['path'].is_a?(Hash)
      if yaml['path']['append'] && yaml['path']['append'].is_a?(Array)
        yaml['path']['append'].each do |path|
          path = File.join(File.dirname(yaml_file), path) if path[0] != '/'
          @path[:append] << ConfigValue.new(path, yaml_file, labels: labels)
        end
      end
      if yaml['path']['prepend'] && yaml['path']['prepend'].is_a?(Array)
        yaml['path']['prepend'].each do |path|
          path = File.join(File.dirname(yaml_file), path) if path[0] != '/'
          @path[:prepend].unshift(ConfigValue.new(path, yaml_file, labels: labels))
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

  def omnipath(include_local: true)
    paths = []

    config_paths = @path.deep_dup
    unless include_local
      config_paths[:prepend]&.reject! { |path| path.has_label?('git_repo') }
      config_paths[:append]&.reject! { |path| path.has_label?('git_repo') }
    end

    paths.push(*config_paths[:prepend].map(&:unwrap)) if config_paths[:prepend]&.any?
    paths.push(*OmniEnv::OMNIPATH) if OmniEnv::OMNIPATH&.any?
    paths.push(*config_paths[:append].map(&:unwrap)) if config_paths[:append]&.any?
    paths.uniq!

    paths
  end

  def path_from_repo
    @path_from_repo ||= begin
      return {} unless OmniEnv.in_git_repo?
      git_repo_path = Pathname.new(OmniEnv.git_repo_root)

      path = [:prepend, :append].map do |key|
        values = @path[key].select { |path| path.has_label?('git_repo') }
        [key, values.map(&:unwrap)]
      end.to_h

      stringify_keys(path)
    end
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
        YAML::load(file)
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

  def import_values(values, file_path: nil, config: nil, labels: nil)
    config = @config&.dup || {} if config.nil?

    values.each do |key, value|
      if value.is_a?(Hash) && config[key.to_s] && config[key.to_s].value.is_a?(Hash)
        import_values(value, file_path: file_path, config: config[key.to_s], labels: labels)
      else
        config[key.to_s] = ConfigValue.new(value, file_path, labels: labels)
      end
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
