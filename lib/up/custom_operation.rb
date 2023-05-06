require_relative '../colorize'
require_relative '../utils'
require_relative 'operation'


class CustomOperation < Operation
  def up
    if met?
      STDERR.puts "# Skipping #{name || config_meet} (already met)".light_yellow
      return
    end

    STDERR.puts "# #{name}".light_blue if name
    meet || run_error(name || config_meet)
  end

  def down
    return unless unmeet_cmd

    unless met?
      STDERR.puts "# Skipping revert of #{name || config_meet} (not met)".light_yellow
      return
    end

    STDERR.puts "# Revert: #{name}".light_blue if name
    unmeet || run_error(name || config_unmeet)
  end

  private

  def name
    config['name']
  end

  def met?
    return false unless met_cmd
    system("#{met_cmd} >/dev/null 2>/dev/null")
  end

  def meet
    command_line(meet_cmd)
  end

  def unmeet
    command_line(unmeet_cmd)
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

