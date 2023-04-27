require_relative 'command'


class OmniCommandWithAliases < OmniCommand
  def initialize(command, aliases)
    @cmd = command
    @aliases = aliases
  end

  def path
    @cmd.path
  end

  def method_missing(method, *args, &block)
    if @cmd.respond_to?(method)
      @cmd.send(method, *args, &block)
    else
      super
    end
  end

  def respond_to_missing?(method, include_private = false)
    @cmd.respond_to?(method, include_private) || super
  end

  def cmds
    @commands ||= [@cmd, *@aliases]
  end

  def cmds_s
    @cmds_s ||= cmds.sort_by(&:cmd).map { |c| c.cmd.join(' ') }
  end

  protected

  def sort_key
    key = @cmd.sort_key

    category = key.first

    cmd = [key.last]
    @aliases.each do |alias_cmd|
      cmd << alias_cmd.sort_key.last
    end if @aliases
    cmd = cmd.sort.first

    [category, cmd]
  end
end
