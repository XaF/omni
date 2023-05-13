require 'singleton'

require_relative 'config'
require_relative 'env'


class OmniOrgs
  include Singleton
  include Enumerable

  def self.method_missing(method, *args, **kwargs, &block)
    if self.instance.respond_to?(method)
      self.instance.send(method, *args, **kwargs, &block)
    else
      super
    end
  end

  def self.respond_to_missing?(method, include_private = false)
    self.instance.respond_to?(method, include_private) || super
  end

  def each(&block)
    @each.each { |org| yield org } if block_given? && !@each.nil?

    @each ||= begin
      each_orgs = []

      OmniEnv::OMNI_ORG.each do |org|
        omniOrg = OmniOrg.new(org)
        yield omniOrg if block_given?

        each_orgs << omniOrg
      end

      # Add the base git path as repo org
      omniOrg = OmniRepoBase.new
      yield omniOrg if block_given?
      each_orgs << omniOrg

      each_orgs
    end

    @each
  end

  def map(&block)
    each.map(&block)
  end

  def select(&block)
    each.select(&block)
  end

  def any?
    each.any?
  end

  def first
    each.first
  end

  def repos(dedup: true)
    seen_paths = Set.new if dedup
    all_repos = []

    OmniOrgs.map(&:path?).uniq.each do |base_path|
      next unless File.directory?(base_path)
      Dir.chdir(base_path) do |dir|
        Dir.glob("**/*/**/.git").each do |path|
          next unless File.directory?(path)

          path = File.dirname(path)
          dir_path = File.join(dir, path)

          next if dedup && !seen_paths.add?(dir_path)

          yield [dir, path, dir_path] if block_given?
          all_repos << [dir, path, dir_path]
        end
      end
    end

    all_repos
  end

  def all_repos
    all_repos = []

    Dir.chdir(OmniEnv::OMNI_GIT) do |dir|
      return [] unless File.directory?(dir)
      Dir.glob("**/*/**/.git").each do |path|
        next unless File.directory?(path)

        path = File.dirname(path)
        dir_path = File.join(dir, path)

        yield [dir, path, dir_path] if block_given?
        all_repos << [dir, path, dir_path]
      end
    end

    all_repos
  end
end


class OmniRepo
  RSYNC_ADDRESS_PATTERN = %r{^(([^@]+@)?([^:]+)):(.*)$}

  attr_reader :uri

  def initialize(path)
    @uri = repo_uri(path)
    raise "Invalid repo path: #{path}" if @uri == nil
  end

  def id
    @id ||= begin
      path = @uri.path
      path.sub!(/\.git$/, '')

      full_repo = path&.sub(%r{^/}, '')
      raise ArgumentError, "Repo address (#{@uri.to_s}) is not complete" if full_repo.nil?

      org_name, repo_name = full_repo.split('/', 2)
      raise ArgumentError, "Repo address (#{@uri.to_s}) is not complete" if org_name.nil? || repo_name.nil?

      "#{@uri.host}:#{org_name}/#{repo_name}"
    end
  end

  def path?(repo = nil)
    r = repo_in_repo(repo)
    r.path.sub!(/\.git$/, '')

    full_repo = r.path&.sub(%r{^/}, '')
    org_name, repo_name = full_repo.split('/', 2) if full_repo

    template_values = {
      host: r.host,
      org: org_name,
      repo: repo_name,
    }

    repo_reldir = Config.repo_path_format % template_values
    repo_absdir = "#{OmniEnv::OMNI_GIT}/#{repo_reldir}"

    if repo.nil?
      # If we didn't pass a repo as argument, we want the root path
      # of the org, so we remove everything after the first "empty"
      # path component and we return the resulting value
      repo_absdir.gsub!(%r{/{2,}.*$}, '')
      return repo_absdir
    end

    repo_absdir.gsub!(%r{/+}, '/')
    repo_absdir.gsub!(%r{/$}, '')

    repo_absdir
  end

  def remote?(repo = nil, force: false)
    uri = repo_in_repo(repo)

    return if !force && ( uri.host.nil? || uri.path.nil? || uri.path.empty? || uri.path == '/')

    # Clean-up the repeated '/' in the path
    uri.path.gsub!(%r{/+}, '/')

    # Now get things as a string
    uri_s = uri.to_s

    # If SSH, apply a few changes to make it rsync-styled
    if uri.scheme == 'ssh'
      uri_s.sub!(%r{^ssh://}, '')
      uri_s.sub!(%r{#{Regexp.escape(uri.path)}$}, ":#{uri.path[1..-1]}")
    end

    uri_s
  end

  def org
    @org
  end

  def org=(org)
    @org = org
  end

  def to_s
    remote?(force: true)
  end

  private

  def repo_in_repo(repo)
    new_uri = @uri.dup
    return new_uri if repo.nil?

    n_repo = OmniRepo.new(repo)

    unless n_repo.uri.path.nil?
      path = n_repo.uri.path.dup
      path = path[1..-1] if n_repo.uri.path.start_with?('/')
      path = path[0..-5] if path.end_with?('.git')

      path = "#{org}/#{path}" unless path.include?('/')

      new_uri.path = "/#{path}.git"

      new_uri.query = n_repo.uri.query
      new_uri.fragment = n_repo.uri.fragment
    end

    unless n_repo.uri.user.nil?
      new_uri.user = n_repo.uri.user
      new_uri.password = n_repo.uri.password
    end

    unless n_repo.uri.host.nil?
      new_uri.host = n_repo.uri.host
      new_uri.port = n_repo.uri.port
      new_uri.query = n_repo.uri.query
      new_uri.fragment = n_repo.uri.fragment
    end

    for key in %i[scheme port query fragment password]
      unless n_repo.uri.send(key).nil?
        new_uri.send("#{key}=", n_repo.uri.send(key))
      end
    end

    new_uri
  end

  def repo_uri(path)
    return URI.parse('') if path.nil? || path.empty?

    # Clean up the path from any trailing spaces
    path = path.strip

    # If the string does contain : or @ and does not contain ://, then
    # assume it is a ssh path and add it right now or the value will be
    # interpreted as path by URI.parse
    if path !~ %r{^[^:]+://}
      path = if path =~ %r{[@:]}
        "ssh://#{path}"
      elsif path =~ %r{\.}
        "https://#{path}"
      else
        path
      end
    end

    # Try parsing the URI without any more modifications
    parsed = begin
      URI.parse(path)
    rescue URI::InvalidURIError
      nil
    end

    # Return the parsed URI if it is valid and has a host or path
    return parsed unless parsed.nil? || (parsed.host.nil? && parsed.path.nil?)

    # Otherwise, try another time after converting the URI from
    # potentially an rsync-formatted URI to a regular one
    path = path.sub(%r{^ssh://}, '')
    path = path.sub(OmniRepo::RSYNC_ADDRESS_PATTERN, '\1/\4')
    path = "ssh://#{path}" unless path =~ %r{^[^:]+://}

    # Try parsing the URI again, but this time if it fails, return nil
    parsed = begin
      URI.parse(path)
    rescue URI::InvalidURIError
      nil
    end

    parsed
  end
end


class OmniRepoBase < OmniRepo
  def initialize
    @uri = URI.parse('')
  end

  def to_s
    'default'
  end
end


class OmniOrg
  attr_reader :path, :repo

  def initialize(path)
    @path = path
    @repo = OmniRepo.new(path)
    @repo.org = File.split(@repo.uri.path)[1]
  end

  def path?(repo = nil)
    @repo.path?(repo)
  end

  def remote?(repo = nil)
    @repo.remote?(repo)
  end

  def to_s
    @repo.to_s
  end

  def repos
    all_repos = []

    Dir.chdir(path?) do |dir|
      Dir.glob("**/.git").each do |path|
        next unless File.directory?(path)

        path = File.dirname(path)
        dir_path = File.join(dir, path)

        yield [dir, path, dir_path] if block_given?
        all_repos << [dir, path, dir_path]
      end
    end

    all_repos
  end
end

