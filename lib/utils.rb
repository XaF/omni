require 'shellwords'
require 'timeout'

require_relative 'env'


def omni_cmd(*cmd)
  raise 'OMNI_CMD_FILE not set' unless OmniEnv::OMNI_CMD_FILE

  cmd = Shellwords.join(*cmd)

  File.open(OmniEnv::OMNI_CMD_FILE, 'a') do |f|
    f.write("#{cmd}\n")
  end
end


def error(msg, cmd: nil)
  cmd = cmd || OmniEnv::OMNI_SUBCOMMAND
  command_failed = cmd ? "#{cmd} command failed:" : 'command failed:'

  STDERR.puts "#{"omni:".light_cyan} #{command_failed.red} #{msg}"

  exit 1
end


def command_line(*cmd, timeout: nil)
  msg = "\n$ #{cmd.join(' ')}".light_black
  msg = "#{msg} #{"(timeout: #{timeout}s)".light_blue}" if timeout
  STDERR.puts msg

  if timeout
    pid = Process.spawn(*cmd)
    begin
      Timeout.timeout(timeout) do
        Process.wait(pid)
        return $?.success?
      end
    rescue Timeout::Error
      Process.kill('TERM', pid)
      false
    end
  else
    system(*cmd)
  end
end
