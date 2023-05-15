require 'json'

require_relative '../cache'
require_relative '../colorize'
require_relative '../utils'
require_relative 'operation'


class HomebrewOperation < Operation
  HOMEBREW_OPERATION_CACHE_KEY = 'homebrew-operation'.freeze

  def up(skip_headers: false)
    return unless brew_installed?

    if met?
      STDERR.puts "# Homebrew dependencies already installed".light_yellow unless skip_headers
      return true
    end

    STDERR.puts "# Install Homebrew dependencies".light_blue unless skip_headers

    required_packages = []
    installed_packages = []
    local_tap_packages = []

    config.each do |pkg|
      pkgname, data = pkg.first
      version = data['version'] if data
      pkgid = "#{pkgname}#{"@#{version}" if version}"
      required_packages << pkgid

      if pkg_installed?(pkgname, version)
        command_line('brew', 'upgrade', pkgname) || run_error("brew upgrade #{pkgname}") unless version
        next
      end

      if version
        local_tap_packages << pkgid
        unless local_tap_formula_exists?(pkgname, version)
          unless tap_exists?
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

      installed_packages << pkgid
      command_line('brew', 'install', pkgid) || run_error("brew install #{pkgid}")
    end

    # Update the cache to save the dependencies installed for this repository
    Cache.exclusive(HOMEBREW_OPERATION_CACHE_KEY) do |cache|
      brew_cache = cache&.value || {}

      local_tap_packages.each do |pkgid|
        brew_cache['local_tap'] ||= []
        brew_cache['local_tap'] << pkgid
        brew_cache['local_tap'].uniq!
      end

      brew_cache['packages'] ||= {}
      required_packages.each do |pkgid|
        brew_cache['packages'][pkgid] ||= {}
        brew_cache['packages'][pkgid]['required_by'] ||= []
        brew_cache['packages'][pkgid]['required_by'] << OmniEnv.git_repo_origin
        brew_cache['packages'][pkgid]['required_by'].uniq!
      end

      installed_packages.each do |pkgid|
        brew_cache['packages'][pkgid] ||= {}
        brew_cache['packages'][pkgid]['installed'] = true
      end

      brew_cache
    end

    !had_errors
  end

  def down
    return unless brew_installed?
    return true unless partial_met?

    STDERR.puts "# Uninstalling Homebrew dependencies".light_blue

    packages = config.map do |pkg|
      pkgname, data = pkg.first
      version = data['version'] if data
      pkgid = "#{pkgname}#{"@#{version}" if version}"

      [pkgid, pkgname, version]
    end

    # Update the cache to save the dependencies installed for this repository
    uninstall_packages = []
    remove_local_tap = false
    Cache.exclusive(HOMEBREW_OPERATION_CACHE_KEY) do |cache|
      brew_cache = cache&.value || {}

      packages.each do |pkgid, pkgname, version|
        next unless brew_cache['packages'].key?(pkgid)

        brew_cache['packages'][pkgid]['required_by'].delete(OmniEnv.git_repo_origin)
        if brew_cache['packages'][pkgid]['required_by'].empty?
          uninstall_packages << [pkgid, pkgname, version] if brew_cache['packages'][pkgid]['installed']
          brew_cache['packages'].delete(pkgid)
        end

        if brew_cache['local_tap']&.include?(pkgid)
          brew_cache['local_tap'].delete(pkgid)

          if brew_cache['local_tap'].empty?
            brew_cache.delete('local_tap')
            remove_local_tap = true
          end
        end
      end

      brew_cache.delete('packages') if brew_cache['packages'].empty?

      brew_cache
    end

    uninstall_packages.each do |pkgid, pkgname, version|
      next unless pkg_installed?(pkgname, version)
      command_line('brew', 'uninstall', pkgid) || run_error("brew uninstall #{pkgid}")
    end

    if remove_local_tap && tap_exists?
      command_line('brew', 'untap', tap_name) || run_error("brew untap #{tap_name}")
    end

    !had_errors
  end

  private

  def brew_installed?
    return @brew_installed unless @brew_installed.nil?
    @brew_installed = system('command -v brew >/dev/null')
  end

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

