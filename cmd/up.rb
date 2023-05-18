#!/usr/bin/env ruby
#
# category: Git commands
# autocompletion: true
# config: up
# opt:--update-user-config:Whether we should handle paths found in the configuration
# opt:--update-user-config:of the repository if any (yes/ask/no); When using \033[3mup\033[0m,
# opt:--update-user-config:the \033[3mpath\033[0m configuration will be copied to the home
# opt:--update-user-config:directory of the user to be loaded on every omni call. When
# opt:--update-user-config:using \033[3mdown\033[0m, the \033[3mpath\033[0m configuration of the
# opt:--update-user-config:repository will be removed from the home directory of the user
# opt:--update-user-config:if it exists \033[90m(default: no)\033[0m
# opt:--trust:Define how to trust the repository (always/yes/no) to run the command.
# help: Sets up or tear down a repository depending on its \033[3mup\033[0m configuration

require_relative '../lib/colorize'
require_relative '../lib/config'
require_relative '../lib/up/bundler_operation'
require_relative '../lib/up/custom_operation'
require_relative '../lib/up/go_operation'
require_relative '../lib/up/homebrew_operation'
require_relative '../lib/up/ruby_operation'
require_relative '../lib/up/operation'
require_relative '../lib/omniorg'
require_relative '../lib/utils'


options = SubcommandOptions({update_user_config: :no, trust: nil}) do |opts, options|
  opts.on(
    "--update-user-config [yes/ask/no]",
    "--handle-path [yes/ask/no]",
    [:yes, :ask, :no],
    "Whether we should import/remove paths found in the repository if any (yes/ask/no)"
  ) do |update_user_config|
    options[:update_user_config] = update_user_config || :ask
  end

  opts.on(
    "--trust [always/yes/no]",
    [:always, :yes, :no],
    "Trust the repository, and run the command without asking for confirmation"
  ) do |trust|
    options[:trust] = trust || :yes
  end
end

error('too many arguments') if ARGV.size > 0
error("can only be run from a git repository") unless OmniEnv.in_git_repo?


class ConfigSection
  attr_reader :section

  def initialize(key, value)
    @section = { key => value }
  end

  def to_s
    @to_s ||= YAML.dump(@section).split("\n").map do |line|
      next if line.chomp == "---"
      line
    end.compact.join("\n    ").green
  end

  def to_h
    @section
  end
end


def trusted_repo?(trust: nil)
  if trust.nil?
    # Get the repository object from the repository origin
    repo = OmniRepo.new(OmniEnv.git_repo_origin)

    # Check if the repository is in a known org
    trust = OmniOrgs.each.any? do |org|
      repo.in_org?(org)
    end

    # If not in a trusted org, check if the repo was already marked as safe
    trust ||= Cache.get('trusted_repositories', nil)&.include?(repo.id) || false

    unless trust
      STDERR.puts "#{"omni:".light_cyan} #{"#{OmniEnv::OMNI_SUBCOMMAND}:".light_yellow} The repository #{repo.id.light_blue} is not in your trusted repositories."

      STDERR.puts "#{"omni:".light_cyan} #{"#{OmniEnv::OMNI_SUBCOMMAND}:".light_yellow} #{"Tip:".bold} if you set #{"OMNI_ORG".italic}, repositories in your organizations are automatically trusted." if OmniOrgs.size == 1

      trust = begin
        UserInteraction.oneof?("Do you want to run #{"omni up".bold} for this repository?", default: 2) do |q|
          q.choice(key: "a", name: "Yes, always (add to trusted repositories)", value: :always)
          q.choice(key: "y", name: "Yes, this time (and ask me everytime)", value: :yes)
          q.choice(key: "n", name: "No", value: :no)
        end
      rescue UserInteraction::StoppedByUserError
        nil
      end
    end
  end

  if trust&.is_a?(Symbol)
    if trust == :always
      Cache.exclusive('trusted_repositories') do |trusted_repositories|
        trusted_repositories ||= []
        trusted_repositories << repo.id
        trusted_repositories.uniq!
        trusted_repositories.sort!
        trusted_repositories
      end
    end

    trust = [:always, :yes].include?(trust)
  end

  unless trust
    STDERR.puts "#{"omni:".light_cyan} #{"#{OmniEnv::OMNI_SUBCOMMAND}:".light_yellow} Skipped running #{"omni up".bold} for this repository."
    return false
  end

  return true
end

def update_path_user_config(config, proceed: false)
  merged_path = {}
  [['append', :push], ['prepend', :unshift]].each do |key, func|
    merged_path[key] = config.dig('path', key).dup || []
    (Config.path_from_repo[key] || []).each do |path|
      merged_path[key].send(func, path) unless merged_path[key].include?(path)
    end
  end
  merged_path.select! { |_, value| !value.empty? }
  merged_path.transform_values! { |value| value.uniq }

  return false if merged_path == (config.dig('path') || {})

  STDERR.puts "#{"omni:".light_cyan} #{"#{OmniEnv::OMNI_SUBCOMMAND}:".light_yellow} The current repository is declaring paths for omni commands."
  STDERR.puts "#{"omni:".light_cyan} #{"#{OmniEnv::OMNI_SUBCOMMAND}:".light_yellow} The following paths are going to be set in your #{"omni".underline} configuration:"
  STDERR.puts "  #{"path:".green}"
  YAML.dump(merged_path).each_line do |line|
    line = line.chomp
    next if line == "---"
    STDERR.puts "    #{line.green}"
  end
  if config.dig('path', 'append') || config.dig('path', 'prepend')
    STDERR.puts "#{"omni:".light_cyan} #{"#{OmniEnv::OMNI_SUBCOMMAND}:".light_yellow} Previous configuration contained:"
    STDERR.puts "  #{"path:".red}"
    YAML.dump(config.dig('path')).each_line do |line|
      line = line.chomp
      next if line == "---"
      STDERR.puts "    #{line.red}"
    end
  end

  proceed = proceed || begin
    UserInteraction.confirm?("Do you want to continue?")
  rescue UserInteraction::StoppedByUserError, UserInteraction::NoMatchError
    false
  end

  if proceed
    STDERR.puts "#{"omni:".light_cyan} #{"#{OmniEnv::OMNI_SUBCOMMAND}:".light_yellow} Updated path user configuration."
    config['path'] = merged_path
    true
  else
    STDERR.puts "#{"omni:".light_cyan} #{"#{OmniEnv::OMNI_SUBCOMMAND}:".light_yellow} Skipped updating path user configuration."
    false
  end
end

def update_suggested_user_config(config, proceed: false)
  suggested_config = Config.suggested_from_repo(unwrap: false)
  current_config = suggested_config.keys.map do |key|
    key = $~[:key] if key =~ ConfigUtils::STRATEGY_REGEX
    value = config.dig(key)
    next if value.nil?
    value = value.respond_to?(:deep_dup) ? value.deep_dup : value.dup
    [key, value]
  end.compact.to_h

  merged_config = ConfigUtils.smart_merge(
    current_config, suggested_config,
    transform: ConfigUtils.method(:transform_path))

  return false if merged_config == current_config

  STDERR.puts "#{"omni:".light_cyan} #{"#{OmniEnv::OMNI_SUBCOMMAND}:".light_yellow} The current repository is suggesting configuration changes."
  STDERR.puts "#{"omni:".light_cyan} #{"#{OmniEnv::OMNI_SUBCOMMAND}:".light_yellow} The following is going to be set in your #{"omni".underline} configuration:"
  YAML.dump(merged_config).each_line do |line|
    line = line.chomp
    next if line == "---"
    STDERR.puts "  #{line.green}"
  end
  if current_config.any?
    STDERR.puts "#{"omni:".light_cyan} #{"#{OmniEnv::OMNI_SUBCOMMAND}:".light_yellow} Previous configuration was:"
    YAML.dump(current_config).each_line do |line|
      line = line.chomp
      next if line == "---"
      STDERR.puts "  #{line.red}"
    end
  end

  unless proceed
    apply = begin
      UserInteraction.oneof?("Do you want to continue?") do |q|
        q.choice(key: "y", name: "Yes, apply the changes", value: :yes)
        q.choice(key: "n", name: "No, skip the changes", value: :no)
        q.choice(key: "s", name: "Split (choose which sections to apply)", value: :split)
      end
    rescue UserInteraction::StoppedByUserError
      nil
    end

    if apply == :yes
      proceed = true
    elsif apply == :split
      choices = merged_config.map { |key, value| ConfigSection.new(key, value) }
      merged_config = begin
        UserInteraction.which_ones?(
          "Which sections do you want to apply?",
          choices,
          default: (1..choices.size).to_a.reverse,
        )
      rescue UserInteraction::StoppedByUserError
        []
      end.map(&:to_h).inject({}, &:merge)
      proceed = merged_config.any?
    end
  end

  if proceed
    STDERR.puts "#{"omni:".light_cyan} #{"#{OmniEnv::OMNI_SUBCOMMAND}:".light_yellow} Updated user configuration."
    merged_config.each { |key, value| config[key] = value }
    true
  else
    STDERR.puts "#{"omni:".light_cyan} #{"#{OmniEnv::OMNI_SUBCOMMAND}:".light_yellow} Skipped updating user configuration."
    false
  end
end

def update_user_config(proceed: false)
  return if OmniEnv::OMNI_SUBCOMMAND == 'down'

  Config.user_config_file(:readwrite) do |config|
    path_updated = update_path_user_config(config, proceed: proceed) if Config.path_from_repo.any?
    suggested_updated = update_suggested_user_config(config, proceed: proceed) if Config.suggested_from_repo.any?

    # Only update the configuration file if something changed
    if path_updated || suggested_updated
      config
    else
      nil
    end
  end
end

def handle_up
  # Prepare all the commands that will need to be run, and check that the configuration is valid
  operations = Config.up.each_with_index.map do |operation, idx|
    operation = { operation => {} } if operation.is_a?(String)
    error("invalid #{'up'.yellow} configuration for operation #{idx.to_s.yellow}") \
      unless operation.is_a?(Hash) && operation.size == 1

    optype = operation.keys.first
    opconfig = operation[optype]

    cls = begin
      Object.const_get("#{optype.capitalize}Operation")
    rescue NameError
      error("invalid #{'up'.yellow} configuration for operation #{idx.to_s.yellow}: unknown operation #{optype.yellow}")
    end

    error("invalid #{'up'.yellow} configuration for operation #{idx.to_s.yellow}: invalid operation #{optype.yellow}") \
      unless cls < Operation

    cls.new(opconfig, index: idx)
  end

  # Run the commands from the git repository root
  Dir.chdir(OmniEnv.git_repo_root) do
    # Convert the subcommand to the operation type
    operation_type = OmniEnv::OMNI_SUBCOMMAND.downcase.to_sym

    # Still block in case operation is unknown
    error("unknown operation #{operation_type.to_s.yellow}") unless [:up, :down].include?(operation_type)

    # In case of being called as `down`, this will run the operations in reverse order
    # in case there are dependencies between them
    operations.reverse! if operation_type == :down

    # Run the operations as long as the up command returns true or nil
    operations.take_while do |operation|
      status = operation.send(operation_type)
      status.nil? || status
    end
  end
end


should_handle_up = Config.respond_to?(:up) && Config.up
should_update_user_config = [:yes, :ask].include?(options[:update_user_config]) && (
  Config.path_from_repo.any? || Config.suggested_from_repo.any?)

if should_handle_up || should_update_user_config
  exit 0 unless trusted_repo?(trust: options[:trust])

  if should_handle_up
    error("invalid #{'up'.yellow} configuration, it should be a list") unless Config.up.is_a?(Array)
    handle_up
  end

  update_user_config(proceed: options[:update_user_config] == :yes) if should_update_user_config
else
  STDERR.puts "#{"omni:".light_cyan} #{"#{OmniEnv::OMNI_SUBCOMMAND}:".light_yellow} No #{'up'.italic} configuration found, nothing to do."
end
