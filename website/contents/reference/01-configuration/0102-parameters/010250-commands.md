---
description: Configuration of the `commands` parameter
---

# `commands`

Commands made available through omni, while the user is in the scope of the configuration file defining those commands.

Any command defined in a global configuration file will be available throughout the whole system. Any command defined in the configuration of a git repository will only be available in that repository. You can check the [configuration custom commands](/reference/custom-commands/configuration) to read more about how commands defined with the `commands` parameter behave.

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

The syntax parameter can take a `parameters` key containing a list of `parameter` objects, and a `groups` key containing a list of `group` objects. If providing a list directly as the syntax parameter, it will be considered as the `parameters` key.

:::info
Some of the configuration options are only relevant when using the argument parser, these are marked below with a start `*`. Others can be helpful in anycase when showing the help for the custom command.

The `groups` key is only useful when using the argument parser.
:::

Each `parameter` object can take the following parameters:

| Parameter        | Type      | Description                                           |
|------------------|-----------|-------------------------------------------------------|
| `name` | string | the name of the parameter |
| `dest`* | string | the name of the variable to store the value of the parameter, if not provided will use a sanitized version of the name |
| `aliases` | string (list) | list of aliases for that parameter |
| `desc` | string | the description/help for the parameter |
| `required` | bool | whether or not this parameter is required |
| `placeholders` | string (list) | the placeholders to show in the help for that parameter; if multiple placeholders are provided, they will be used one after the other depending on the `num_values` configuration |
| `type` | string | the type of the parameter, can be one of `str`, `int`, `float`, `bool`, `flag`, `counter`, `enum(vals, ...)` or `array/<type>` for any of those except `flag` and `counter`. See below for more details on the types. |
| `default` | string | the default value for the parameter |
| `num_values` | string | the number of values that the parameter can take. This can take ranges in the format `..max` (open), `..=max` (closed), `min..`, `min..max` (half-open), `min..=max` (closed) |
| `delimiter`* | char | the delimiter to use when splitting the values of the parameter; when specified, the argument parser will split each value by this delimiter and provide them as separate values |
| `last`* | bool | to indicate the last, or final, positional argument, which is only able to be accessed via the `--` syntax (i.e. `$ prog args -- last_arg`) |
| `leftovers`* | bool | everything that follows that parameter should be captured by it, as if the user had used a `--` |
| `allow_hyphen_values`* | bool | allow values that start with a hyphen to be considered as values, and not as options |
| `allow_negative_numbers`* | bool | allow negative numbers to be considered as values; similar to `allow_hyphen_values` but only allow for digits after the hyphen |
| `group_occurrences` | bool | Group occurrences of parameters together when they take multiple values and can be repeated |
| `requires`* | string (list) | list of parameters that are required when this parameter is present |
| `conflicts_with`* | string (list) | list of parameters that cannot be used with this parameter |
| `required_without`* | string (list) | this parameter is required when any of the parameters in the list is not present |
| `required_without_all`* | string (list) | this parameter is required when all of the parameters in the list are not present |
| `required_if_eq`* | map | this parameter is required when the parameter in the map is equal to the value in the map |
| `required_if_eq_all`* | map | this parameter is required when all the parameters in the map are equal to the value in the map |

Each `group`* object can take the following parameters:

| Parameter        | Type      | Description                                           |
|------------------|-----------|-------------------------------------------------------|
| `name` | string | the name of the group |
| `parameters` | string (list) | list of parameters that are part of that group |
| `required` | bool | whether or not this group is required |
| `requires` | string (list) | list of groups that are required when this group is present |
| `conflicts_with` | string (list) | list of groups that cannot be used with this group |

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
