require 'json'

require_relative '../cache'
require_relative '../colorize'
require_relative '../utils'
require_relative 'operation'


class PacmanOperation < Operation
  PACMAN_OPERATION_CACHE_KEY = 'pacman-operation'.freeze

  def up(skip_headers: false)
    return unless pacman_installed?

    if met?
      STDERR.puts "# pacman dependencies already installed".light_yellow unless skip_headers
      return true
    end

    STDERR.puts "# Install pacman dependencies".light_blue unless skip_headers

    required_packages = []
    installed_packages = []

    config.each do |pkg|
      pkgname, data = pkg.first
      version = data['version'] if data
      pkgid = "#{pkgname}#{"=#{version}" if version}"
      required_packages << pkgid

      next if pkg_installed?(pkgname, version)

      installed_packages << pkgid
    end

    if installed_packages.any?
      command_line('sudo', 'pacman', '-Sy', '--noconfirm', *installed_packages) || \
        run_error("pacman -Sy --noconfirm #{installed_packages.join(' ')}")
    end

    # Update the cache to save the dependencies installed for this repository
    Cache.exclusive(PACMAN_OPERATION_CACHE_KEY) do |cache|
      pacman_cache = cache&.value || {}

      pacman_cache['packages'] ||= {}
      required_packages.each do |pkgid|
        pacman_cache['packages'][pkgid] ||= {}
        pacman_cache['packages'][pkgid]['required_by'] ||= []
        pacman_cache['packages'][pkgid]['required_by'] << OmniEnv.git_repo_origin
        pacman_cache['packages'][pkgid]['required_by'].uniq!
      end

      installed_packages.each do |pkgid|
        pacman_cache['packages'][pkgid] ||= {}
        pacman_cache['packages'][pkgid]['installed'] = true
      end

      pacman_cache
    end

    !had_errors
  end

  def down
    return unless pacman_installed?
    return true unless partial_met?

    STDERR.puts "# Uninstalling pacman dependencies".light_blue

    packages = config.map do |pkg|
      pkgname, data = pkg.first
      version = data['version'] if data
      pkgid = "#{pkgname}#{"=#{version}" if version}"

      [pkgid, pkgname, version]
    end

    # Update the cache to save the dependencies installed for this repository
    uninstall_packages = []
    remove_local_tap = false
    Cache.exclusive(PACMAN_OPERATION_CACHE_KEY) do |cache|
      pacman_cache = cache&.value || {}

      packages.each do |pkgid, pkgname, version|
        next unless pacman_cache['packages'].key?(pkgid)

        pacman_cache['packages'][pkgid]['required_by'].delete(OmniEnv.git_repo_origin)
        if pacman_cache['packages'][pkgid]['required_by'].empty?
          uninstall_packages << [pkgid, pkgname, version] if pacman_cache['packages'][pkgid]['installed']
          pacman_cache['packages'].delete(pkgid)
        end
      end

      pacman_cache.delete('packages') if pacman_cache['packages'].empty?

      pacman_cache
    end

    uninstall_packages.select! { |_, pkgname, version| pkg_installed?(pkgname, version) }
    uninstall_packages.map! { |pkgid, _, _| pkgid }

    command_line('sudo', 'pacman', '-Rs', '--noconfirm', *uninstall_packages) || \
      run_error("pacman -Rs --noconfirm #{uninstall_packages.join(' ')}")

    !had_errors
  end

  private

  def pacman_installed?
    return @pacman_installed unless @pacman_installed.nil?
    @pacman_installed = system('command -v pacman >/dev/null')
  end

  def met?
    config.all? do |pkg|
      pkgname, data = pkg.first
      version = data['version'] if data

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

  def pkg_installed?(pkgname, version = nil)
    pkgid = "#{pkgname}#{"=#{version}" if version}"

    @pkg_installed ||= {}
    return @pkg_installed[pkgid] unless @pkg_installed[pkgid].nil?

    @pkg_installed[pkgid] = system("pacman -Q #{pkgid} >/dev/null 2>&1")
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

