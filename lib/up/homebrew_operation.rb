require 'json'

require_relative '../colorize'
require_relative '../utils'
require_relative 'operation'


class HomebrewOperation < Operation
  def up
    if met?
      STDERR.puts "# Homebrew dependencies already installed".light_yellow
      return
    end

    STDERR.puts "# Install Homebrew dependencies".light_blue

    config.each do |pkg|
      pkgname, data = pkg.first
      version = data['version'] if data

      next if pkg_installed?(pkgname, version)

      if version
        unless local_tap_formula_exists?(pkgname, version)
          unless tap_exists?(pkgname)
            # This creates a new local tap so we can fetch the formula of the
            # requested version into it
            command_line("brew tap | grep -q #{tap_name} || brew tap-new #{tap_name}") || \
              (run_error("brew tap-new #{tap_name}") && next)
            tap_exists!
          end

          # Extract the requested version into the local tap
          command_line('brew', 'extract', '--version', version, pkgname, tap_name) || \
            (run_error("brew extract #{pkgname}") && next)
        end
      end

      pkgid = "#{pkgname}#{"@#{version}" if version}"
      command_line('brew', 'install', pkgid) || run_error("brew install #{pkgid}")
    end
  end

  def down
    return unless partial_met?

    STDERR.puts "# Uninstalling Homebrew dependencies".light_blue

    config.each do |pkg|
      pkgname, data = pkg.first
      version = data['version'] if data

      next unless pkg_installed?(pkgname, version)

      pkgid = "#{pkgname}#{"@#{version}" if version}"
      command_line('brew', 'uninstall', pkgid) || run_error("brew uninstall #{pkgid}")
    end
  end

  private

  def met?
    config.all? do |pkg|
      pkgname, data = pkg.first
      version = data['version'] if data

      # If no version is specified, this is not met since
      # we might need to update the package
      return false unless version

      pkg_installed?(pkgname, version)
    end
  end

  def partial_met?
    config.any? do |pkg|
      pkgname, data = pkg.first
      version = data['version'] if data

      pkg_installed?(pkgname, version)
    end
  end

  def tap_exists?
    return @tap_exists unless @tap_exists.nil?
    @tap_exists = tap_info['installed'] || false
  end

  def tap_exists!
    @tap_exists = true
  end

  def tap_name
    'omni/local'
  end

  def tap_info
    @tap_info ||= JSON.parse(`brew tap-info #{tap_name} --json`.chomp)&.first || {}
  end

  def local_tap_formula_exists?(pkgname, version)
    return false unless tap_info
    tap_info['formula_names'].include?("#{tap_name}/#{pkgname}@#{version}")
  end

  def pkg_installed?(pkgname, version = nil)
    pkgid = "#{pkgname}#{"@#{version}" if version}"

    @pkg_installed ||= {}
    return @pkg_installed[pkgid] unless @pkg_installed[pkgid].nil?

    @pkg_installed[pkgid] = if version && !local_tap_formula_exists?(pkgname, version)
      false
    else
      system("brew list #{pkgid} >/dev/null 2>&1")
    end

    @pkg_installed[pkgid]
  end

  def check_valid_operation!
    @config = [{ config => {} }] if config.is_a?(String)

    @config = config.map do |key, value|
      { key => value }
    end if config.is_a?(Hash)

    config_error("expecting array, got #{config}") unless config.is_a?(Array)

    @config.map! do |item|
      item = { item => {} } if item.is_a?(String)
      config_error("expecting hash, got #{item}") unless item.is_a?(Hash)

      key, value = item.first
      value = { 'version' => value } if value.is_a?(String)
      check_params(allowed_params: ['version'], check_against: value)

      { key => value }
    end
  end
end

