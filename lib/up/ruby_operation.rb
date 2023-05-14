require_relative '../colorize'
require_relative '../utils'
require_relative 'operation'


class RubyOperation < Operation
  def up
    STDERR.puts "# Install Ruby #{config['version']}".light_blue

    if is_installed?
      STDERR.puts "# Ruby #{ruby_version} is already installed".light_green
      set_ruby_local_version!
      return true
    end

    puts "# `-> Ruby #{ruby_version} will be installed".light_blue
    install_ruby! && set_ruby_local_version!

    !had_errors
  end

  def down
    nil
  end

  private

  def ruby_version
    @ruby_version ||= begin
      # Refresh the list of available ruby versions
      ruby_build_local_dir = File.expand_path('~/.rbenv/plugins/ruby-build')
      if File.directory?(ruby_build_local_dir) && File.directory?(File.join(ruby_build_local_dir, '.git'))
        `git -C "#{ruby_build_local_dir}" pull`
      end

      # List all ruby versions
      rubies = `rbenv install --list-all`.split("\n").map(&:strip)

      # Select only the versions that start with the prefix, and that
      # contain only numbers and dots
      rubies.select! { |ruby| ruby =~ /\A#{Regexp.escape(config['version'])}(\.[0-9\.]*)?\z/ }

      # We have an issue if there are no matching versions
      error("No ruby version found matching #{config['version']}") if rubies.empty?

      # The expected ruby version is the highest matching version number returned,
      # and since `rbenv install --list-all` returns versions in ascending order,
      # the last one is the highest
      rubies.last
    end
  end

  def is_installed?
    @is_installed ||= begin
      # Make sure we have a ruby version before checking if it is installed
      _ = ruby_version

      # Check if the ruby version is already installed
      `rbenv versions --bare 2>/dev/null`.split("\n").map(&:strip).any? do |ruby|
        ruby == ruby_version
      end
    end
  end

  def install_ruby!
    download_ruby! || build_ruby!
  end

  def download_ruby!
    # Check if 'rbenv download' is available
    return false unless system('rbenv download --list >/dev/null 2>&1')

    # Download the ruby version, and rehash rbenv to make it available
    return true if command_line('rbenv', 'download', ruby_version, '&&', 'rbenv', 'rehash')

    # If we got here, the download failed
    STDERR.puts "# `-> Could not download pre-built binary, will have to build it ⏳".light_blue
    false
  end

  def build_ruby!
    return true if command_line(
      'rbenv', 'install',
      # '--verbose',
      '--skip-existing',
      ruby_version,
      env: build_ruby_env,
      chdir: File.expand_path('~'),
    )

    run_error("rbenv install #{ruby_version}")
    false
  end

  def build_ruby_env
    @build_ruby_env ||= begin
      # Override all the BUNDLE/BUNDLER/GEM and some RUBY env variables
      env = ENV.each_pair.map do |key, value|
        next unless key =~ /\A(BUNDLE|BUNDLER|GEM|RBENV)_/ || ['RUBYOPT', 'RUBYLIB'].include?(key)
        [key, nil]
      end.compact.to_h

      # Disable the installation of the ri and rdoc documentation
      env['RUBY_CONFIGURE_OPTS'] = '--disable-install-doc'

      # Return the cleaned env
      env
    end
  end

  def set_ruby_local_version!
    Dir.chdir(OmniEnv::GIT_REPO_ROOT) do
      output = `rbenv local #{ruby_version} 2>&1`
      $?.success? || run_error("Failed to set ruby version to #{ruby_version}: #{output}")
    end
  end

  def check_valid_operation!
    @config = { 'version' => config.to_s } if config.is_a?(String) || config.is_a?(Numeric)
    config_error("expecting hash, got #{config}") unless config.is_a?(Hash)

    check_params(required_params: ['version'])
  end
end

