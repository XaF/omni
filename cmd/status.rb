#!/usr/bin/env ruby
#
# category: General
# help: Show status of omni
# help:
# help: This will show the configuration that omni is loading when
# help: it is being called, which includes the configuration files
# help: but also the current cached information.
# help:
# help: \e[1m\e[3mUsage\e[0m\e[1m: omni status\e[0m

require_relative '../lib/colorize'
require_relative '../lib/cache'
require_relative '../lib/config'
require_relative '../lib/env'
require_relative '../lib/omniorg'


def recursive_dump(hash, indent: 0, valid_keys: nil)
  hash.each do |key, value|
    if value.is_a?(Hash)
      STDERR.puts "#{" " * indent}#{key}:"
      recursive_dump_hash(value, indent + 2)
    else
      key_s = if valid_keys && !valid_keys.has_key?(key)
        key.to_s.red
      else
        key.to_s.cyan
      end

      STDERR.puts "#{" " * indent}#{key_s}: #{value.inspect}"
    end
  end
end


def expires_in(expires_at, round = 2)
    expires_in = expires_at - Time.now

    if expires_in > 60 * 60 * 24 * 7
      value = (expires_in / (60 * 60 * 24 * 7))
      unit = 'week'
    elsif expires_in > 60 * 60 * 24
      value = (expires_in / (60 * 60 * 24))
      unit = 'day'
    elsif expires_in > 60 * 60
      value = (expires_in / (60 * 60))
      unit = 'hour'
    elsif expires_in > 60
      value = (expires_in / 60)
      unit = 'minute'
    else
      value = expires_in
      unit = 'second'
    end

    value = value.round(round)
    "#{value} #{unit}#{value > 1 ? 's' : ''}"
end


STDERR.puts "#{"omni".bold} - omnipotent tool"

STDERR.puts ""
STDERR.puts "Shell integration".bold
if OmniEnv::OMNI_CMD_FILE
  STDERR.puts "  #{'loaded'.green}"
else
  STDERR.puts "  #{'not loaded'.red}"
end


STDERR.puts ""
STDERR.puts "Configuration".bold
recursive_dump(Config.config, indent: 2, valid_keys: Config.default_config)

STDERR.puts ""
STDERR.puts "Loaded configuration files".bold
if Config.loaded_files.any?
  Config.loaded_files.each do |file|
    STDERR.puts "  - #{file}"
  end
else
  STDERR.puts "  #{'none'.red}"
end

STDERR.puts ""
STDERR.puts "Cache".bold
if Cache.any?
  Cache.each do |key, value|
    val = "#{value.value}"

    unless value.expires_at.nil?
      val << " (expires in ~#{expires_in(value.expires_at, 0)})".light_black
    end

    STDERR.puts "  #{key.to_s.cyan}: #{val}"
  end
else
  STDERR.puts "  #{'none'.red}"
end

STDERR.puts ""
STDERR.puts "Environment".bold
recursive_dump(OmniEnv.env, indent: 2)

STDERR.puts ""
STDERR.puts "Git Orgs".bold
if OmniOrgs.any?
  OmniOrgs.each do |org|
    STDERR.puts "  #{org.to_s} #{"(#{org.path?})".light_black}"
  end
else
  STDERR.puts "  #{'none'.red}"
end
