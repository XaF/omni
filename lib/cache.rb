require 'json'
require 'time'
require 'singleton'

require_relative 'env'
require_relative 'config'


class Cache
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

  def initialize
    read_cache
  end

  def get(key, default = nil)
    @cache[key]&.value || default
  end

  def set(key, value, expires_in: nil, expires_at: nil)
    expires_at = Time.now + expires_in if expires_at.nil? && !expires_in.nil?
    @cache[key] = CachedValue.new(value, expires_at: expires_at)
    write_cache
  end

  def each(&block)
    @cache.reject { |_, value| value.expired? }.each(&block)
  end

  def any?
    @cache.reject { |_, value| value.expired? }.any?
  end

  def clear
    @cache = {}
    write_cache
  end

  private

  def write_cache
    @cache.reject! { |_, value| value.expired? }
    File.write(Config.cache_file, @cache.to_json)
  end

  def read_cache
    @cache ||= {}
    return unless File.exist?(Config.cache_file)

    cache = JSON.parse(File.read(Config.cache_file))

    cache.each do |key, value|
      cached_value = CachedValue.new(
        value['value'],
        created_at: Time.parse(value['created_at']),
        expires_at: Time.parse(value['expires_at']),
      )

      @cache[key] = cached_value unless cached_value.expired?
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
