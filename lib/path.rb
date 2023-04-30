require 'singleton'

require_relative 'command'
require_relative 'command_collection'
require_relative 'config'
require_relative 'env'
require_relative 'makefile_path'


class OmniPath
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
    @each.each { |command| yield command } if block_given? && @each

    @each ||= begin
      # By using this data structure, we make sure that no two commands
      # can have the same command call name; the second one will be
      # ignored.
      each_commands = OmniCommandCollection.new

      paths = OmniEnv::OMNIPATH.dup
      if Config.enable_git_repo_commands && OmniEnv.in_git_repo?
        paths.unshift(File.join(OmniEnv.git_repo_root, '.omni', 'cmd'))
      end

      paths.each do |dirpath|
        next unless File.directory?(dirpath) && File.readable?(dirpath)

        Dir.chdir(dirpath) do |dir|
          Dir.glob('**/*').each do |filepath|
            next unless File.executable?(filepath) && File.file?(filepath)

            # remove the path from the command as prefix
            cmd = filepath.split('/')

            # Create and yield the OmniCommand object
            omniCmd = OmniCommand.new(cmd, File.join(dirpath, filepath))
            yield omniCmd if block_given?

            each_commands << omniCmd
          end
        end
      end

      MakefilePath.each do |omniCmd|
        yield omniCmd if block_given?
        each_commands << omniCmd
      end if Config.enable_makefile_commands

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
