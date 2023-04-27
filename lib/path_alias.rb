require 'singleton'

require_relative 'command_collection'
require_relative 'command_alias'
require_relative 'path'


class OmniPathWithAliases
  include Singleton
  include Enumerable

  def self.method_missing(method, *args, &block)
    if self.instance.respond_to?(method)
      self.instance.send(method, *args, &block)
    else
      super
    end
  end

  def self.respond_to_missing?(method, include_private = false)
    self.instance.respond_to?(method, include_private) || super
  end

  def each(&block)
    @each ||= begin
      # Prepare all the data we initially need to merge commands with their aliases
      paths = Set.new
      realpaths = OmniPath.map do |command|
        paths.add(command.path)
        [Pathname.new(command.path).realpath.to_s, command]
      end

      # Find all the aliases for each command
      aliases = {}
      realpaths.
        select { |path, command| path != command.path && paths.include?(path) }.
        each do |path, command|
          aliases[path] ||= []
          aliases[path] << command
        end

      commands = OmniCommandCollection.new
      realpaths.each do |path, command|
        next if aliases[path]&.include?(command)
        commands << OmniCommandWithAliases.new(command, aliases[path])
      end

      commands.sort!
      commands
    end

    @each.each { |command| yield command } if block_given?

    @each
  end

  def map(&block)
    each.map { |command| yield command }
  end

  def max_command_length
    @max_command_length ||= map { |cmd| cmd.cmds_s.join(', ').length }.max
  end
end

