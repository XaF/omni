require 'json'

require_relative '../cache'
require_relative '../colorize'
require_relative '../utils'
require_relative 'operation'


class DnfOperation < Operation
  DNF_OPERATION_CACHE_KEY = 'dnf-operation'.freeze

  def up(skip_headers: false)
    return unless dnf_installed?

    if met?
      STDERR.puts "# dnf dependencies already installed".light_yellow unless skip_headers
      return true
    end

    STDERR.puts "# Install dnf dependencies".light_blue unless skip_headers

    required_packages = []
    installed_packages = []

    config.each do |pkg|
      pkgname, data = pkg.first
      version = data['version'] if data
      pkgid = "#{pkgname}#{"-#{version}" if version}"
      required_packages << pkgid

      next if pkg_installed?(pkgname, version)

      installed_packages << pkgid
      command_line('sudo', 'dnf', '--assumeyes', 'install', pkgid) || run_error("dnf --assumeyes install #{pkgid}")
    end

    # Update the cache to save the dependencies installed for this repository
    Cache.exclusive(DNF_OPERATION_CACHE_KEY) do |cache|
      dnf_cache = cache&.value || {}

      dnf_cache['packages'] ||= {}
      required_packages.each do |pkgid|
        dnf_cache['packages'][pkgid] ||= {}
        dnf_cache['packages'][pkgid]['required_by'] ||= []
        dnf_cache['packages'][pkgid]['required_by'] << OmniEnv.git_repo_origin
        dnf_cache['packages'][pkgid]['required_by'].uniq!
      end

      installed_packages.each do |pkgid|
        dnf_cache['packages'][pkgid] ||= {}
        dnf_cache['packages'][pkgid]['installed'] = true
      end

      dnf_cache
    end

    !had_errors
  end

  def down
    return unless dnf_installed?
    return true unless partial_met?

    STDERR.puts "# Uninstalling dnf dependencies".light_blue

    packages = config.map do |pkg|
      pkgname, data = pkg.first
      version = data['version'] if data
      pkgid = "#{pkgname}#{"-#{version}" if version}"

      [pkgid, pkgname, version]
    end

    # Update the cache to save the dependencies installed for this repository
    uninstall_packages = []
    remove_local_tap = false
    Cache.exclusive(DNF_OPERATION_CACHE_KEY) do |cache|
      dnf_cache = cache&.value || {}

      packages.each do |pkgid, pkgname, version|
        next unless dnf_cache['packages'].key?(pkgid)

        dnf_cache['packages'][pkgid]['required_by'].delete(OmniEnv.git_repo_origin)
        if dnf_cache['packages'][pkgid]['required_by'].empty?
          uninstall_packages << [pkgid, pkgname, version] if dnf_cache['packages'][pkgid]['installed']
          dnf_cache['packages'].delete(pkgid)
        end
      end

      dnf_cache.delete('packages') if dnf_cache['packages'].empty?

      dnf_cache
    end

    uninstall_packages.each do |pkgid, pkgname, version|
      next unless pkg_installed?(pkgname, version)
      command_line('sudo', 'dnf', '--assumeyes', 'remove', pkgid) || run_error("dnf --assumeyes remove #{pkgid}")
    end

    !had_errors
  end

  private

  def dnf_installed?
    return @dnf_installed unless @dnf_installed.nil?
    @dnf_installed = system('command -v dnf >/dev/null')
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
    pkgid = "#{pkgname}#{"-#{version}" if version}"

    @pkg_installed ||= {}
    return @pkg_installed[pkgid] unless @pkg_installed[pkgid].nil?

    @pkg_installed[pkgid] = system("dnf list --installed #{pkgid} >/dev/null 2>&1")
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

