require_relative '../colorize'
require_relative '../utils'
require_relative 'operation'


class AsdfOperation < Operation
  def up
    STDERR.puts "# Install #{tool} #{config['version']}".light_blue

    install_asdf!
    unless is_asdf_installed?
      STDERR.puts "# asdf is not installed and we did not seem to be able to install it".light_red
      return false
    end

    install_tool!
    unless is_tool_installed?
      STDERR.puts "# #{tool} plugin for asdf is not installed and we did not seem to be able to install it".light_red
      return false
    end

    if is_tool_version_installed?
      STDERR.puts "# #{tool} #{tool_version} is already installed".light_green
      set_tool_local_version!
      return !had_errors
    end

    puts "# `-> #{tool} #{tool_version} will be installed".light_blue
    install_tool_version! && set_tool_local_version!

    !had_errors
  end

  def down
    nil
  end

  def is_asdf_installed?
    @asdf_installed ||= system('command -v asdf >/dev/null 2>&1')
  end

  def is_tool_installed?
    @tool_installed ||= `asdf plugin list 2>/dev/null`.split("\n").map(&:strip).include?(tool)
  end

  private

  def tool
    config['tool']
  end

  def asdf_path
    File.expand_path('~/.asdf')
  end

  def install_asdf!
    unless is_asdf_installed?
      unless File.directory?(asdf_path)
        git_clone = command_line('git', 'clone', 'https://github.com/asdf-vm/asdf.git', asdf_path)
        return unless git_clone
      end

      # Update environment variables
      ENV['PATH'] = "#{asdf_path}/bin:#{asdf_path}/shims:#{ENV['PATH']}"
      return unless is_asdf_installed?

      # And take advantage of the shell integration to update
      # the current shell environment of the user
      omni_cmd('export PATH="$HOME/.asdf/bin:$HOME/.asdf/shims:$PATH"')
      case OmniEnv.user_shell
      when 'fish'
        omni_cmd('source ~/.asdf/asdf.fish')
      when 'zsh', 'bash'
        omni_cmd('source ~/.asdf/asdf.sh')
      end
    end

    # Make sure we're using the last stable version
    unless system('asdf update >/dev/null 2>&1')
      STDERR.puts "# Could not update asdf to the latest version".light_red
    end
  end

  def install_tool!
    return if is_tool_installed?

    STDERR.puts "# Installing #{tool} plugin for asdf".light_blue

    if system("asdf plugin add #{tool} >/dev/null")
      STDERR.puts "# `-> you can set a global version with: asdf global #{tool} <version>".light_green
    end
  end

  def tool_version
    @tool_version ||= begin
      # Refresh the list of available versions
      `asdf plugin update #{tool}`

      # List all versions for the tool
      available_versions = `asdf list all #{tool}`.split("\n").map(&:strip)

      # Select only the versions that start with the prefix, and that
      # contain only numbers and dots; in case latest is specified, we
      # only want to match versions that are only numbers and dots
      version_regex = if config['version'] == 'latest'
        /\A[0-9\.]+\z/
      else
        /\A#{Regexp.escape(config['version'])}(\.[0-9\.]*)?\z/
      end
      available_versions.select! { |version| version_regex.match?(version) }

      # We have an issue if there are no matching versions
      error("No #{tool} version found matching #{config['version']}") if available_versions.empty?

      # The expected tool version is the highest matching version number returned,
      # and since `asdf list all` returns versions in ascending order,
      # the last one is the highest
      available_versions.last
    end
  end

  def is_tool_version_installed?
    @tool_version_installed ||= system("asdf list #{tool} #{tool_version} >/dev/null 2>&1")
  end

  def install_tool_version!
    return true if command_line(
      'asdf', 'install', tool, tool_version,
      chdir: File.expand_path('~'),
    )

    run_error("asdf install #{tool} #{tool_version}")
    false
  end

  def set_tool_local_version!
    Dir.chdir(OmniEnv::GIT_REPO_ROOT) do
      output = `asdf local #{tool} #{tool_version} 2>&1`
      $?.success? || error("Failed to set #{tool} version to #{tool_version}: #{output}")
    end
  end

  def check_valid_operation!
    @config = { 'tool' => config.to_s } if config.is_a?(String) || config.is_a?(Numeric)
    config_error("expecting hash, got #{config}") unless config.is_a?(Hash)

    # In case the version is not specified, we will use the latest
    @config['version'] ||= 'latest'

    check_params(required_params: ['tool', 'version'])
  end
end


class AsdfOperationTool < AsdfOperation
  private

  def tool
    raise RuntimeError, 'tool is not defined'
  end

  def check_valid_operation!
    @config = { 'version' => config.to_s } if config.is_a?(String) || config.is_a?(Numeric)
    config_error("expecting hash, got #{config}") unless config.is_a?(Hash)

    # In case the version is not specified, we will use the latest
    @config['version'] ||= 'latest'

    check_params(required_params: ['version'])
  end
end
