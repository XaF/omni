require_relative '../colorize'
require_relative '../utils'
require_relative 'operation'


class CustomOperation < Operation
  def up
    if met?
      STDERR.puts "# Skipping #{name || meet_cmd} (already met)".light_yellow
      return true
    end

    STDERR.puts "# #{name}".light_blue if name
    meet || run_error(name || meet_cmd)

    !had_errors
  end

  def down
    return unless unmeet_cmd

    unless met?
      STDERR.puts "# Skipping revert of #{name || unmeet_cmd} (not met)".light_yellow
      return true
    end

    STDERR.puts "# Revert: #{name}".light_blue if name
    unmeet || run_error(name || unmeet_cmd)

    !had_errors
  end

  private

  def name
    config['name']
  end

  def met?
    return false unless met_cmd
    system(*wrap_cmd(met_cmd), ">/dev/null 2>&1")
  end

  def meet
    command_line(meet_cmd, wrap_with_shell: true)
  end

  def unmeet
    command_line(unmeet_cmd, wrap_with_shell: true)
  end

  def wrap_cmd(cmd)
    wrapped_cmd = ['bash', '-c', cmd]
    wrapped_cmd.unshift('shadowenv', 'exec', '--') if OmniEnv.shadowenv?
    wrapped_cmd
  end

  def met_cmd
    config['met?']
  end

  def meet_cmd
    config['meet']
  end

  def unmeet_cmd
    config['unmeet']
  end

  def check_valid_operation!
    config_error("expecting hash, got #{config}") unless config.is_a?(Hash)

    check_params(required_params: ['meet'], allowed_params: ['met?', 'name', 'unmeet'])
  end
end

