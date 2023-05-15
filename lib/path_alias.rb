require 'singleton'

require_relative 'command_collection'
require_relative 'command_alias'
require_relative 'path'


class OmniPathWithAliases
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
    @each ||= begin
      # Prepare all the data we initially need to merge commands with their aliases
      paths = Set.new
      realpaths = OmniPath.map.each_with_object({}) do |command, hash|
        paths.add(command.path)

        realpath = begin
          Pathname.new(command.path).realpath.to_s
        rescue Errno::ENOENT
          command.path
        end

        hash[realpath] ||= []
        hash[realpath] << command
      end

      # Now convert that into commands
      commands = OmniCommandCollection.new
      realpaths.each do |path, aliases|
        # aliases.sort_by! do |aliascmd|
          # path = aliascmd.path.dup
          # resolved_path = File.realpath(path)
          # num_symlinks = 0
          # while path != resolved_path
            # num_symlinks += 1
            # path = File.dirname(path)
            # resolved_path = File.realpath(path)
          # end
          # num_symlinks
        # end
        commands << OmniCommandWithAliases.new(aliases.first, aliases[1..-1])
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

