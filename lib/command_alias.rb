require_relative 'command'


class OmniCommandWithAliases
  def initialize(command, aliases)
    @cmd = command
    @aliases = aliases
  end

  def method_missing(method, *args, **kwargs, &block)
    if @cmd.respond_to?(method)
      @cmd.send(method, *args, **kwargs, &block)
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

  def <=>(other)
    sort_key <=> other.sort_key
  end

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
