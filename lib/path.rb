require 'find'

require_relative 'command'
require_relative 'command_collection'
require_relative 'env'
require_relative 'makefile_path'


class OmniPath
  include Enumerable

  @@instance = nil

  def self.instance
    @@instance ||= OmniPath.new
  end

  def self.each(&block)
    self.instance.each(&block)
  end

  def self.map(&block)
    self.instance.map(&block)
  end

  def self.find(&block)
    self.instance.find(&block)
  end

  def self.select(&block)
    self.instance.select(&block)
  end

  def self.sorted(&block)
    self.instance.sorted(&block)
  end

  def self.max_command_length
    self.instance.max_command_length
  end

  def each(&block)
    @each.each { |command| yield command } if block_given? && @each

    @each ||= begin
      # By using this data structure, we make sure that no two commands
      # can have the same command call name; the second one will be
      # ignored.
      each_commands = OmniCommandCollection.new

      OmniEnv::OMNIPATH.each do |dirpath|
        next unless File.directory?(dirpath)

        Find.find(dirpath) do |filepath|
          next unless File.executable?(filepath) && File.file?(filepath)

          # remove the path from the command as prefix
          cmd = filepath.sub(/^#{Regexp.escape(dirpath)}\//, '').split('/')

          # Create and yield the OmniCommand object
          omniCmd = OmniCommand.new(cmd, filepath)
          yield omniCmd if block_given?

          each_commands << omniCmd
        end
      end

      MakefilePath.each do |omniCmd|
        yield omniCmd if block_given?
        each_commands << omniCmd
      end

      each_commands
    end

    @each
  end

  def map(&block)
    return unless block_given?

    commands = []
    each do |command|
      commands << yield(command)
    end
    commands
  end

  def sorted(&block)
    @sorted ||= each.to_a.sort

    @sorted.each { |command| yield command } if block_given?

    @sorted
  end

  def max_command_length
    @max_command_length ||= map(&:length).max
  end
end
