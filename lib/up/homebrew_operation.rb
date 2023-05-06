require_relative '../colorize'
require_relative '../utils'
require_relative 'operation'


class HomebrewOperation < Operation
  def up
    STDERR.puts "# Installing Homebrew dependencies".light_blue

    tap_exists = false

    config.each do |pkg|
      pkgname, data = pkg.first
      version = data['version'] if data

      if version
        # Add new tap to fetch the version if needed
        unless tap_exists
          command_line("brew tap | grep -q #{tap_name} || brew tap-new #{tap_name}") || \
            (run_error("brew tap-new #{tap_name}") && next)
          tap_exists = true
        end

        # Extract the requested version
        command_line('brew', 'extract', '--version', version, pkgname, tap_name) || \
          (run_error("brew extract #{pkgname}") && next)

        # Change the package name so we install the right version
        pkgname = "#{pkgname}@#{version}"
      end

      command_line('brew', 'install', pkgname) || run_error("brew install #{pkgname}")
    end
  end

  def down
    STDERR.puts "# Uninstalling Homebrew dependencies".light_blue

    config.each do |pkg|
      pkgname, data = pkg.first
      version = data['version'] if data
      pkgname = "#{pkgname}@#{version}" if version

      command_line('brew', 'uninstall', pkgname) || run_error("brew uninstall #{pkgname}")
    end
  end

  private

  def tap_name
    'omni/local'
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

