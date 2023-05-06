#!/usr/bin/env ruby
#
# category: Git commands
# config: up
# help: Sets up or tear down a repository depending on its \e[3mup\e[0m configuration
# help:
# help: \e[1m\e[3mUsage\e[0m\e[1m: omni \e[36m[\e[0m\e[1mup\e[36m|\e[0m\e[1mdown\e[36m]\e[0m

require_relative '../lib/colorize'
require_relative '../lib/config'
require_relative '../lib/up/bundler_operation'
require_relative '../lib/up/custom_operation'
require_relative '../lib/up/homebrew_operation'
require_relative '../lib/up/operation'
require_relative '../lib/utils'


error('too many arguments') if ARGV.size > 0
error("can only be run from a git repository") unless OmniEnv.in_git_repo?
unless Config.respond_to?(:up) && Config.up
  STDERR.puts "#{"omni:".light_cyan} #{"#{OmniEnv::OMNI_SUBCOMMAND}:".light_yellow} No #{'up'.italic} configuration found, nothing to do."
  exit 0
end
error("invalid #{'up'.yellow} configuration, it should be a list") unless Config.up.is_a?(Array)

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
  if OmniEnv::OMNI_SUBCOMMAND == 'up'
    # Run the operations in the provided order
    operations.each(&:up)
  else
    # In case of being called as `down`, this will also
    # run the operations in reverse order in case there
    # are dependencies between them
    operations.reverse.each(&:down)
  end
end
