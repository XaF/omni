require_relative '../path'


class LookupCommand
  def self.autocomplete(comp_cword, argv)
    commands = OmniPath.each.to_a

    # Prepare until which word we need to match
    match_pos = if comp_cword >= 0
      comp_cword
    else
      argv.length - 1
    end

    # If we have full subcommands in the argv, we only want to
    # show the subcommands that match what's already provided,
    # but since we can delegate autocomplete, if this is not a
    # direct match, we will try reducing the constraints one
    # argument at a time and see if we can still get to something
    skip_elems = 0
    match = nil

    until match&.any? || skip_elems > match_pos
      skip_elems += 1

      match = commands.select do |omniCmd|
        omniCmd.cmd[0..match_pos - skip_elems] == argv[0..match_pos - skip_elems]
      end if match_pos > 0
    end
    commands = match if match&.any?

    if skip_elems == 1
      # For the last value in argv, we need to use more of a
      # matching with the start of the command
      match_last_val = commands.select do |omniCmd|
        omniCmd.cmd[match_pos]&.start_with?(argv[match_pos])
      end if argv.length > match_pos
      commands = match_last_val if match_last_val&.any?
    end

    if commands.length == 1 && commands[0].cmd.length <= match_pos
      omniCmd = commands[0]

      # If we get there, let's try and delegate calling --complete
      # to the underlying function in case it provides more
      # autocompletion...

      # Open the file and check the headers to see if it supports
      # autocompletion
      if omniCmd.autocompletion?
        # Set the environment variables that we need to pass to the
        # subcommand
        ENV['COMP_CWORD'] = (comp_cword - omniCmd.cmd.length + 1).to_s

        # Call the subcommand with the --complete flag, we delegate
        # the answer to it
        omniCmd.autocomplete(*argv)
        return
      end
    end

    # If skip_elems is greater than 1, it means that we had to
    # go backward in our matching, and since we got here, it means
    # that we either didn't delegate the autocompletion process
    # to a subcommand or that the subcommand returned without
    # providing any autocompletion; we can thus exit here
    return if skip_elems > 1

    # Extract the values at the expected position
    commands.map! { |omniCmd| omniCmd.cmd[match_pos] }
    commands.compact!
    commands.uniq!
    commands.sort!

    # Print the commands, one per line
    commands.each do |cmd|
      puts cmd
    end
  end
end
