require 'singleton'

require_relative 'command_collection'
require_relative 'command_alias'
require_relative 'config_command'
require_relative 'makefile_command'
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
      realpaths = OmniPath.map do |command|
        paths.add(command.path)

        realpath = begin
          Pathname.new(command.path).realpath.to_s
        rescue Errno::ENOENT
          command.path
        end

        # If the command provides a line number, append it to the realpath
        # This is used for commands like `makefile` which are all computed from
        # the same file, but we want to display them separately, except if
        # we find two symlinks to the same Makefile
        realpath += ":#{command.lineno}" if command.respond_to?(:lineno)

        [realpath, command]
      end

      # Merge realpaths into a map of realpath => [commands...]
      realpaths = realpaths.each_with_object({}) do |(path, command), hash|
        hash[path] ||= []
        hash[path] << command
      end

      # Now convert that into commands
      commands = OmniCommandCollection.new
      realpaths.each do |path, aliases|
        # For special commands all computed from the same file, we do not want to
        # merge aliases into a single help line, but rather display them separately
        # if there are no real way to know which commands are exactly the same
        if aliases.first.is_a?(ConfigCommand)
          aliases.each do |cmd|
            commands << OmniCommandWithAliases.new(cmd, [])
          end
          next
        end

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

