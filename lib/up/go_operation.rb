require_relative '../colorize'
require_relative '../utils'
require_relative 'operation'
require_relative 'homebrew_operation'


class GoOperation < Operation
  def up
    STDERR.puts "# Install Go #{config['version']}".light_blue

    install_goenv!
    unless is_goenv_installed?
      STDERR.puts "# Goenv is not installed and we did not seem to be able to install it".light_red
      return false
    end

    if is_go_version_installed?
      STDERR.puts "# Go #{go_version} is already installed".light_green
      set_go_local_version!
      return !had_errors
    end

    puts "# `-> Go #{go_version} will be installed".light_blue
    install_go! && set_go_local_version!

    !had_errors
  end

  def down
    nil
  end

  private

  def is_goenv_installed?
    system('command -v goenv >/dev/null 2>&1')
  end

  def install_goenv!
    already_installed = is_goenv_installed?

    # # Check if we can simply use a homebrew operation to install goenv
    # homebrew_goenv = HomebrewOperation.new('goenv').up(skip_headers: true)

    # We just wanted to make sure to register the `up` in case we are
    # handling things with homebrew, so we will simply return true here
    # if goenv was already installed
    return true if already_installed

    # if homebrew_goenv
      # # If installed through brew, we know the `goenv` command should
      # # be made available right away if the path of the user is properly
      # # configured, however the goenv init call is not made automatically
      # omni_cmd('export PATH="$(goenv root)/shims:$PATH"')
      # omni_cmd('eval "$(goenv init -)"')
      # return true
    # end

    # # If the homebrew operation failed, we can stop here
    # return false if homebrew_goenv == false

    # If the homebrew operation is neither truthy or false, it means
    # that the operation was not applicable, so we need to install goenv
    # manually
    goenv_path = File.expand_path('~/.goenv')

    git_clone = command_line(
      'git', 'clone', 'https://github.com/syndbg/goenv.git',
      goenv_path, '--depth', '1')
    return false unless git_clone

    # Update environment variables
    ENV['GOENV_ROOT'] = goenv_path
    ENV['PATH'] = "#{goenv_path}/bin:#{goenv_path}/shims:#{ENV['PATH']}"

    # And take advantage of the shell integration to update
    # the current shell environment of the user
    omni_cmd('export GOENV_ROOT="$HOME/.goenv"')
    omni_cmd('export PATH="$GOENV_ROOT/bin:$GOENV_ROOT/shims:$PATH"')
    omni_cmd('export GOENV_DISABLE_GOPATH=1')
    omni_cmd('eval "$(goenv init -)"')
  end

  def go_version
    @go_version ||= begin
      # Refresh the list of available ruby versions
      goenv_local_dir = File.expand_path('~/.goenv')
      if File.directory?(goenv_local_dir) && File.directory?(File.join(goenv_local_dir, '.git'))
        `git -C "#{goenv_local_dir}" pull`
      end

      # List all go versions
      goes = `goenv install --list`.split("\n").map(&:strip)

      # Select only the versions that start with the prefix, and that
      # contain only numbers and dots; in case latest is specified, we
      # only want to match versions that are only numbers and dots
      version_regex = if config['version'] == 'latest'
        /\A[0-9\.]+\z/
      else
        /\A#{Regexp.escape(config['version'])}(\.[0-9\.]*)?\z/
      end
      goes.select! { |go| version_regex.match?(go) }

      # We have an issue if there are no matching versions
      error("No go version found matching #{config['version']}") if goes.empty?

      # The expected go version is the highest matching version number returned,
      # and since `goenv install --list` returns versions in ascending order,
      # the last one is the highest
      goes.last
    end
  end

  def is_go_version_installed?
    @is_installed ||= begin
      # Make sure we have a go version before checking if it is installed
      _ = go_version

      # Check if the go version is already installed
      `goenv versions --bare 2>/dev/null`.split("\n").map(&:strip).any? do |go|
        go == go_version
      end
    end
  end

  def install_go!
    return true if command_line(
      'goenv', 'install',
      # '--verbose',
      '--skip-existing',
      go_version,
      chdir: File.expand_path('~'),
    )

    run_error("goenv install #{go_version}")
    false
  end

  def set_go_local_version!
    Dir.chdir(OmniEnv::GIT_REPO_ROOT) do
      output = `goenv local #{go_version} 2>&1`
      $?.success? || error("Failed to set go version to #{go_version}: #{output}")
    end
  end

  def check_valid_operation!
    @config = { 'version' => config.to_s } if config.is_a?(String) || config.is_a?(Numeric)
    config_error("expecting hash, got #{config}") unless config.is_a?(Hash)

    # In case the version is not specified, we will use the latest
    @config['version'] ||= 'latest'

    check_params(required_params: ['version'])
  end
end

class GolangOperation < GoOperation; end
