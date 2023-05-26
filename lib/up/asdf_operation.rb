require_relative '../colorize'
require_relative '../utils'
require_relative 'operation'


class AsdfOperation < Operation
  def up
    STDERR.puts "# Install #{tool} #{config['version']}".light_blue

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

  def shadowenv?
    @shadowenv ||= system("command -v shadowenv >/dev/null")
  end

  def is_tool_installed?
    @tool_installed ||= `#{asdf_bin} plugin list 2>/dev/null`.split("\n").map(&:strip).include?(tool)
  end

  def shadowenv(env)
    file_name = "600_#{tool}_#{tool_version}.lisp"

    contents = <<~LISP
    (provide "#{tool}" "#{tool_version}")

    (when (null (env/get "OMNI_DATA_HOME"))
      (env/set "OMNI_DATA_HOME"
        (path-concat
          (if (or (null (env/get "XDG_DATA_HOME")) (not (string-prefix-p "/" (env/get "XDG_DATA_HOME"))))
            (path-concat (env/get "HOME") ".local/share")
            (env/get "XDG_DATA_HOME"))
          "omni")))

    (let ((tool_path (path-concat (env/get "OMNI_DATA_HOME") "asdf" "installs" "#{tool}" "#{tool_version}")))
      (env/prepend-to-pathlist "PATH" (path-concat tool_path "bin")))
    LISP

    env.write(file_name, contents)
  end

  private

  def tool
    config['tool']
  end

  def asdf_path
    @asdf_path ||= begin
      asdf_data_dir = ENV['ASDF_DATA_DIR']
      return asdf_data_dir if asdf_data_dir

      omni_data_home = ENV['OMNI_DATA_HOME']
      unless omni_data_home && !omni_data_home.nil?
        xdg_data_home = ENV['XDG_DATA_HOME']
        xdg_data_home = "#{ENV['HOME']}/.local/share" unless xdg_data_home && xdg_data_home.start_with?('/')

        omni_data_home = "#{xdg_data_home}/omni"
      end

      asdf_data_dir = "#{omni_data_home}/asdf"
      ENV['ASDF_DATA_DIR'] = asdf_data_dir

      asdf_data_dir
    end
  end

  def asdf_bin
    "#{asdf_path}/bin/asdf"
  end

  def tool_path
    "#{asdf_path}/installs/#{tool}/#{tool_version}"
  end

  def install_tool!
    return if is_tool_installed?

    STDERR.puts "# Installing #{tool} plugin for asdf".light_blue

    if system("#{asdf_bin} plugin add #{tool} >/dev/null")
      unless system("#{asdf_bin} global #{tool} system >/dev/null")
        #STDERR.puts "# Could not set global version for #{tool}; you can do it with: asdf global #{tool} system".light_red
      end
    end
  end

  def tool_version
    @tool_version ||= begin
      # Refresh the list of available versions
      `#{asdf_bin} plugin update #{tool}`

      # List all versions for the tool
      available_versions = `#{asdf_bin} list all #{tool}`.split("\n").map(&:strip)

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

  def tool_version_major
    tool_version.split('.')[0]
  end

  def tool_version_minor
    tool_version.split('.')[0..1].join('.')
  end

  def is_tool_version_installed?
    @tool_version_installed ||= system("#{asdf_bin} list #{tool} #{tool_version} >/dev/null 2>&1")
  end

  def install_tool_version!
    return true if command_line(
      asdf_bin, 'install', tool, tool_version,
      chdir: File.expand_path('~'),
    )

    run_error("#{asdf_bin} install #{tool} #{tool_version}")
    false
  end

  def set_tool_local_version!
    return if shadowenv?
    Dir.chdir(OmniEnv::GIT_REPO_ROOT) do
      output = `#{asdf_bin} local #{tool} #{tool_version} 2>&1`
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
