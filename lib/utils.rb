require 'shellwords'
require 'timeout'

require_relative 'env'


def error(msg, cmd: nil, print_only: false)
  cmd = cmd || OmniEnv::OMNI_SUBCOMMAND
  command_failed = cmd ? "#{cmd} command failed:" : 'command failed:'

  STDERR.puts "#{"omni:".light_cyan} #{command_failed.red} #{msg}"

  exit 1 unless print_only
end


def omni_cmd(*cmd)
  error("unable to propagate shell changes, please setup omni's shell integration") unless OmniEnv::OMNI_CMD_FILE

  cmd = Shellwords.join(*cmd)

  File.open(OmniEnv::OMNI_CMD_FILE, 'a') do |f|
    f.write("#{cmd}\n")
  end
end


def command_line(*cmd, timeout: nil, chdir: nil, context: nil)
  Dir.chdir(chdir) do
    return command_line(*cmd, timeout: timeout, context: context)
  end if chdir

  msg = +""
  msg << "#{context.light_blue} " if context
  msg << "$ #{cmd.join(' ')}".light_black
  msg << " #{"(timeout: #{timeout}s)".light_blue}" if timeout
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
ensure
  STDERR.puts unless chdir
end


class UserInterraction
  class Error < StandardError; end
  class NoMatchError < Error; end

  class StoppedByUserError < Error; end
  class InterruptedError < StoppedByUserError; end
  class RefusedError < StoppedByUserError; end

  def self.confirm?(message = "Proceed?", extra = nil, default_yes: true)
    require 'tty-prompt'

    confirm = begin
      TTY::Prompt.new.method(default_yes ? :yes? : :no?).
        call("#{"omni:".light_cyan} #{message.yellow}#{" #{extra}" if extra}")
    rescue TTY::Reader::InputInterrupt
      # Just a line return to make it look nicer, since we get here
      # in case of interrupt, and the prompt doesn't do it
      puts
      raise InterruptedError
    end

    confirm = !confirm unless default_yes
    raise RefusedError unless confirm
    true
  end

  def self.which_ones?(message = "Which ones?", choices, **options)
    require 'tty-prompt'

    options = {
      default: nil,
      show_help: :always,
      echo: false,
      quiet: true,
    }.merge(options)

    choices = begin
      TTY::Prompt.new.multi_select(
        "#{"omni:".light_cyan} #{message.yellow}",
        choices,
        **options,
      )
    rescue TTY::Reader::InputInterrupt
      # Just a line return to make it look nicer, since we get here
      # in case of interrupt, and the prompt doesn't do it
      puts
      raise InterruptedError
    end

    raise RefusedError if choices.empty?
    return choices
  end

  def self.did_you_mean?(available, search)
    # We do require here because we don't want to load it
    # if we don't need it, as it's a bit heavy
    require 'fuzzy_match'
    require 'amatch'

    # We want to use amatch to make string similarity calculations
    # in a C extension, which is faster than the pure ruby version
    FuzzyMatch.engine = :amatch

    # We want to find and sort the available commands by similarity
    # to the one that was requested
    fuzzy = FuzzyMatch.new(available)
    matching_commands = fuzzy.find_all(search)

    # If we don't have any matching command, we can just exit early
    raise NoMatchError if matching_commands.empty?

    # If we don't have a tty, we can't prompt the user, so we
    # just print the first matching command and exit
    unless STDOUT.tty?
      STDERR.puts "#{"omni:".light_cyan} #{"Did you mean?".yellow} #{matching_commands.first}"
      return
    end

    # If we get there, we want to prompt the user with a list or
    # a yes/no question, in order to run the command that the user
    # wanted to run in the first place
    require 'tty-prompt'

    # If we only have one matching command, we can offer it as a
    # yes/no question instead of a list, as there is not much
    # of any other choice
    return matching_commands.first \
      if matching_commands.length == 1 && \
        confirm?("Did you mean?", matching_commands.first)

    # If we get there, we have multiple matching commands, so we
    # want to prompt the user with a list of commands to choose from
    begin
      return TTY::Prompt.new
        .select("#{"omni:".light_cyan} #{"Did you mean?".yellow}", matching_commands)
    rescue TTY::Reader::InputInterrupt
      # Just a line return to make it look nicer, since we get here
      # in case of interrupt, and the prompt doesn't do it
      puts
      raise InterruptedError
    end
  end
end


# This is a wrapper around TTY::ProgressBar, which will only
# initialize it if STDOUT is a TTY, otherwise it will just
# ignore all calls to it
class TTYProgressBar
  def initialize(*args, **kwargs)
    return unless STDOUT.tty?

    require 'tty-progressbar'

    @bar = TTY::ProgressBar.new(*args, **kwargs)
  end

  def method_missing(method, *args, **kwargs, &block)
    return unless STDOUT.tty?

    @bar.send(method, *args, **kwargs, &block)
  end
end
