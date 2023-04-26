class OmniCommandCollection < Array
  def push(command)
    return if find { |cmd| cmd.cmd == command.cmd }
    super(command)
  end

  def <<(command)
    return if find { |cmd| cmd.cmd == command.cmd }
    super(command)
  end
end
