---
description: Configuration of the `commands` parameter
---

# `commands`

Commands made available through omni, while the user is in the scope of the configuration file defining those commands.

Any command defined in a global configuration file will be available throughout the whole system. Any command defined in the configuration of a git repository will only be available in that repository.

## Parameters

| Parameter        | Type      | Description                                           |
|------------------|-----------|-------------------------------------------------------|
| `aliases` | string (list) | list of aliases for that command |
| `desc` | string | the description of the command that will be used in `omni help`. This can be on multiple lines, in which case the first paragraph (until the first empty line) will be shown in `omni help`, while the rest of the help message will be shown when calling `omni help <command>`. |
| `run` | multiline string | the command to run when the command is being called. This will be called through `bash -c` and can thus receive any kind of bash scripting, or call to an executable file. |
| `category` | string (list) | comma-separated or actual list of categories, organized hierarchically from the least significative to the most significative |
| `dir` | string | path to the directory from which to execute the command, relative to the location of the configuration file, and needs to be a subdirectory |
| `subcommands` | [`commands`](commands) (map) | Subcommands of that command; the name of those commands will be prefixed by the name of the current command (e.g. command `main` and subcommand `sub` would create a command `main sub`) |
| `syntax` | [`syntax`](#syntax) | Define the parameters that the command can take. This will be used when calling `omni help <command>`. |

### Syntax

The syntax parameter takes a list of `parameter` objects. Each `parameter` object can take the following parameters:

| Parameter        | Type      | Description                                           |
|------------------|-----------|-------------------------------------------------------|
| `name` | string | the name of the parameter |
| `desc` | string | the description/help for the parameter |
| `required` | bool | whether or not this parameter is required |

## Example

```yaml
commands:

  # The real minimum viable example to create a command
  hello-world:
    run: echo "Hello world!"

  # Example of command to run tests, both parameters
  # are optional as the command can be run without any
  # arguments.
  run-tests:
    syntax:
      - file: The specific test file to execute
      - args...: Any other options to pass to the test
    desc: "Run the tests for this project"
    run: |
      if [[ $# -eq 0 ]]; then
        bundle exec rake test
      else
        bundle exec ruby -W0 -Itest "$@"
      fi

  # Example of command to generate a random number, both
  # parameters are mandatory as we error out if they are
  # missing, so we set required to `true`.
  random-number:
    syntax:
      - name: min
        desc: Minimum value
        required: true
      - name: max
        desc: Maximum value
        required: true
    desc: "Generates a random number and prints it"
    run: |
      min=${1:?Missing minimum value}
      max=${2:?Missing maximum value}
      random_number=$((min + RANDOM % (max - min + 1)))
      echo $random_number

  # A command with alternative ways to be called
  # Can be called as `omni main`, `omni alt1` or `omni alt2`
  main:
    run: echo "Hello!"
    aliases:
      - alt1
      - alt2

  # And for this command, we want subcommands
  # Can be called as `omni root`
  root:
    run: echo "This is root"
    subcommands:
      # Can be called as `omni root child`
      child:
        run: echo "This is child"
        subcommands:
          # Can be called as `omni root child grandchild`
          grandchild:
            run: echo "This is grandchild"
      # Can be called as `omni root child2`
      child2:
        run: echo "This is child2"
```
