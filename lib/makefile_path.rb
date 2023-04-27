require 'singleton'

require_relative 'makefile_command'


class MakefilePath
  include Singleton
  include Enumerable

  TARGET_REGEX=%r{^([a-zA-Z_0-9\-\/\/]+):(.*?##\s*(.*))?$}

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
      each_commands = []

      locate_files.each do |makefile|
        extract_targets(makefile) do |target, extra|
          omniCmd =  MakefileCommand.new(
            target,
            makefile,
            **extra,
          )

          yield omniCmd if block_given?

          each_commands << omniCmd
        end
      end

      each_commands
    end

    @each
  end

  private

  def locate_files
    @makefiles ||= begin
      makefiles = []

      current_dir = Dir.pwd

      # If we are in a git repository, we stop searching when
      # reaching the top level
      top_level = `git rev-parse --show-toplevel 2>/dev/null`.chomp
      top_level = Dir.home if top_level.empty?

      begin
        while Dir.pwd != '/'
          Dir.entries(Dir.pwd).each do |filename|
            next unless filename =~ /^(GNU)?Makefile(\..*)?$/
            makefiles << File.join(Dir.pwd, filename)
          end

          break if Dir.pwd == top_level

          Dir.chdir('..')
        end
      ensure
        Dir.chdir(current_dir)
      end

      makefiles
    end
  end

  def extract_targets(filepath)
    File.open(filepath, 'r') do |file|
      category = nil

      file.each_line.with_index do |line, lineno|
        category = $1 if line =~ /^##@\s*(.*)$/
        next unless line =~ TARGET_REGEX

        target = $1
        help = $3

        yield target, {help: help, lineno: lineno, category: category}
      end
    end
  end
end
