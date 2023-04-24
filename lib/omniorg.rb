require_relative 'env'


class OmniOrgs
  include Enumerable

  @@instance = nil

  def self.instance
    @@instance ||= OmniOrgs.new
  end

  def self.each(&block)
    self.instance.each(&block)
  end

  def self.any?
    self.instance.any?
  end

  def self.first
    self.instance.first
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

      each_orgs
    end

    @each
  end

  def any?
    each.any?
  end

  def first
    each.first
  end
end


class OmniRepo
  RSYNC_ADDRESS_PATTERN = %r{^(([^@]+@)?([^:]+)):(.*)$}

  attr_reader :uri

  def initialize(path)
    @uri = repo_uri(path)
    raise "Invalid repo path: #{path}" if @uri == nil
  end

  def path?(repo = nil)
    r = repo_in_repo(repo)
    r.path.sub!(/\.git$/, '')

    return "#{OmniEnv::OMNI_GIT}/#{r.host}#{r.path}"
  end

  def remote?(repo = nil)
    uri = repo_in_repo(repo)

    return if uri.host.nil? || uri.path.nil? || uri.path.empty?

    uri_s = uri.to_s
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

  private

  def repo_in_repo(repo)
    new_uri = @uri.dup
    return new_uri if repo.nil?

    n_repo = OmniRepo.new(repo)

    unless n_repo.uri.path.nil?
      path = n_repo.uri.path.dup
      path = path[1..-1] if n_repo.uri.path.start_with?('/')
      path = path[0..-5] if path.end_with?('.git')

      unless path.include?('/')
        raise "Org unknown and repo path #{repo} does not include an org" if org.nil?
        path = "#{org}/#{path}"
      end

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
    parsed = begin
      URI.parse(path)
    rescue URI::InvalidURIError
      nil
    end

    return parsed unless parsed.nil? || (parsed.host.nil? && parsed.path.nil?)

    path = "ssh://#{path}" unless path =~ %r{^[^:]+://}
    path.sub!(OmniOrg::RSYNC_ADDRESS_PATTERN, '\1/\4')

    begin
      URI.parse(path)
    rescue URI::InvalidURIError
      nil
    end
  end
end


class OmniOrg
  RSYNC_ADDRESS_PATTERN = %r{^(([^@]+@)?([^:]+)):(.*)$}

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
end

