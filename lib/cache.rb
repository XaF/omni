require 'json'
require 'time'
require 'singleton'

require_relative 'env'
require_relative 'config'


class Cache
  include Singleton
  include Enumerable

  def self.method_missing(method, *args, **kwargs, &block)
    return self.instance.send(method, *args, **kwargs, &block) if self.instance.respond_to?(method)
    super
  end

  def self.respond_to_missing?(method, include_private = false)
    self.instance.respond_to?(method, include_private) || super
  end

  def initialize
  end

  def get(key, default = nil)
    cache[key]&.value || default
  end

  def set(key, value, expires_in: nil, expires_at: nil)
    exclusive(key, expires_in: expires_in, expires_at: expires_at) do |key_value|
      value
    end
  end

  def each(&block)
    cache.reject { |_, value| value.expired? }.each(&block)
  end

  def any?
    cache.reject { |_, value| value.expired? }.any?
  end

  def clear
    exclusive do |cache|
      {}
    end
  end

  def exclusive(key = nil, expires_in: nil, expires_at: nil, &block)
    cache_file(File::LOCK_EX) do |file, cache|
      # Yield the cache
      yield_value = if key
        cache[key]
      else
        cache
      end
      write = yield yield_value
      return write unless write

      expires_at = Time.now + expires_in if expires_at.nil? && !expires_in.nil?
      write_cache = if key
        write = CachedValue.new(write, expires_at: expires_at) unless write.is_a?(CachedValue)
        cache[key] = write
        cache
      else
        write.map do |key, value|
          value = CachedValue.new(value, expires_at: expires_at) unless value.is_a?(CachedValue)
          [key, value]
        end.to_h
      end

      file.rewind
      file.write(write_cache.to_json)
      file.flush
      file.truncate(file.pos)

      if key
        write_cache[key]
      else
        write_cache
      end
    end
  end

  def shared(key = nil)
    cache_file(File::LOCK_SH) do |_, cache|
      if key
        yield cache[key] if block_given?
        cache[key]
      else
        yield cache if block_given?
        cache
      end
    end
  end

  private

  def cache
    shared
  end

  def cache_file(lock_type = File::LOCK_SH, &block)
    FileUtils.mkdir_p(File.dirname(Config.cache['path']))
    File.open(Config.cache['path'], File::RDWR|File::CREAT, 0644) do |file|
      # Put a shared lock on the file
      file.flock(lock_type)

      # Load the cache
      cache = begin
        JSON.parse(file.read)
      rescue JSON::ParserError
        {}
      end
      cache = cache.map do |key, value|
        cached_value = CachedValue.new(
          value['value'],
          created_at: Time.parse(value['created_at']),
          expires_at: value['expires_at'].nil? ? nil : Time.parse(value['expires_at']),
        )
        [key, cached_value] unless cached_value.expired?
      end.compact.to_h

      yield file, cache
    end
  end
end


class CachedValue
  attr_reader :expires_at, :created_at

  def initialize(value, created_at: nil, expires_at: nil)
    @value = value
    @created_at = created_at || Time.now
    @expires_at = expires_at
  end

  def value
    expired? ? nil : @value
  end

  def expire_at
    @expires_at
  end

  def expired?
    return false if @expires_at.nil?
    @expires_at < Time.now
  end

  def to_json(*args)
    {
      value: @value,
      created_at: @created_at,
      expires_at: @expires_at,
    }.to_json(*args)
  end
end
