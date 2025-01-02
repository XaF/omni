---
sidebar_position: 1
description: Metadata available for custom commands from path
---

# Metadata

Omni supports a number of metadata for custom commands provided through the omnipath, telling omni how to behave, or simply show help for that command.

Commands metadata can be provided either through a dedicated metadata file, colocated with the executable file providing the command, or through metadata headers added directly to the executable file, if not in binary format.

## Metadata headers

To be read properly, omni headers need to:
- Be right at the top of the file, in the header comment with only comment lines preceding it
- Not have lines that are not headers between headers

Omni headers need to be written in the following format:
```bash
# <header>:<value>
```

:::tip Binary files
If the tools that you want to make accessible as omni commands are binary files, it is recommended that you use [a metadata file](#metadata-file) instead.

Alternatively, you can  ensure those binary files are in a different directory of your repository, e.g. `.compiled/`, than the path you want to add to the `omnipath`, e.g. `cmd/`. You can then add a small bash wrapper in the `cmd/` directory that calls your binary behind the scene, while defining omni headers:

```bash
#!/usr/bin/env bash
#
# <headers>

# This script's directory
DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"

# Run the binary
exec "${DIR}/../.compiled/rng" "$@"
```

Omni can also take care of updating the binary with the last updates in the code using a [`custom` operation](/reference/configuration/parameters/up/custom) for `omni up`.
:::

### `category`

The category header indicates the category in which the command needs to be organized. This is particularly useful for a clean `omni help`. The value of that header is a comma-separated list of strings, which are levels of categorization (category, subcategory, subsubcategory).

This can be provided as follows:
```bash
# category: General, Generators
```

Which would categorize the command in the *General* category, in the *Generators* subcategory.

### `autocompletion`

The autocompletion header indicates that the command supports autocompletion and that any autocompletion request received by omni for that command should be forwarded to it. Any other value than `true` is ignored and will be considered as `false`, as is done by default.

This can be provided as follows:
```bash
# autocompletion: true
```

Which would instruct omni to forward autocompletion requests to the path command. See [autocompletion](autocompletion) for how to handle autocompletion in a path command.

### `argparser`

The argparser header indicates that the command uses the argument parser. This means that omni will parse arguments and provide them as environment variables. Any other value than `true` is ignored and will be considered as `false`, as is done by default.

This can be provided as follows:
```bash
# argparser: true
```

Which would instruct omni to parse arguments and provide them as environment variables. See [argument parser](argument-parser) for how to handle arguments once parsed by omni.

### `sync_update`

The sync update header indicates that if the repository requires an update, it should be done synchronously before running the command. This is particularly useful if the command depends on some environment setup from the repository that could change often. Any other value than `true` is ignored and will be considered as `false`, as is done by default.

This can be provided as follows:
```bash
# sync_update: true
```

Which would instruct omni to update the repository synchronously before running the command, if an update is required.

### `arg` and `opt`

The `arg` and `opt` headers allows to define respectively required arguments and optional parameters for the command. These will be shown when running `omni help <command>`, and can be parsed by omni if setting enabling the argument parser. When using the `arg` or `opt` header, you need to define the argument name or format, and the description/help for that argument. The argument name can be a value starting with two dashes for a long parameter (e.g. `--min`) or a single dash for a short parameter (e.g. `-m`), or again without any dash for a positional parameter (e.g. `min`). When using short or long parameters, you can specify multiple of them by comma-separating them. You can also specify a custom placeholder which will be shown in the help message instead of the default, capitalized parameter.

You can also define a number of options between the argument name and the description, in the format `<key>: <value>`, separated by colons.
This can be provided as follows:
```bash
# arg: name: Name to give to the number
# arg: -m, --min MIN_PLACEHOLDER: type=int: Minimum value for the random number
```

The accepted configuration parameters for options and arguments are the following:

| Parameter | Description | Example |
|-----------|-------------|---------|
| `dest` | the name of the variable to store the value of the parameter, if not provided will use a sanitized version of the name | `arg: name: dest=num_name: xxx` |
| `type` | the type of the parameter, can be one of `str`, `int`, `float`, `bool`, `flag`, `counter`, `enum(vals, ...)` or `array/<type>` for any of those except `flag` and `counter`. See below for more details on the types. | `arg: min: type=int` |
| `default` | the default value for the parameter | `arg: min: default=0` |
| `num_values` | the number of values that the parameter can take. This can take ranges in the format `..max` (open), `..=max` (closed), `min..`, `min..max` (half-open), `min..=max` (closed) | `arg: vals: num_values=1..` |
| `delimiter` | the delimiter to use when splitting the values of the parameter; when specified, the argument parser will split each value by this delimiter and provide them as separate values | `arg: vals: delimiter=,` |
| `last` | to indicate the last, or final, positional argument, which is only able to be accessed via the `--` syntax (i.e. `$ prog args -- last_arg`) | `arg: last: true` |
| `leftovers` | everything that follows that parameter should be captured by it, as if the user had used a `--` | `arg: rest: leftovers=true` |
| `allow_hyphen_values` | allow values that start with a hyphen to be considered as values, and not as options | `arg: val: allow_hyphen_values=true` |
| `allow_negative_numbers`* | bool | allow negative numbers to be considered as values; similar to `allow_hyphen_values` but only allow for digits after the hyphen | `arg: val: allow_negative_numbers=true` |
| `group_occurrences` | Group occurrences of parameters together when they take multiple values and can be repeated | `arg: val: group_occurrences=true` |
| `requires` | list of parameters that are required when this parameter is present | `arg: val3: requires=val1 val2` |
| `conflicts_with` | list of parameters that cannot be used with this parameter | `arg: val3: conflicts_with=val1 val2` |
| `required_without` | this parameter is required when any of the parameters in the list is not present | `arg: val3: required_without=val1 val2` |
| `required_without_all` | this parameter is required when all of the parameters in the list are not present | `arg: val3: required_without_all=val1 val2` |
| `required_if_eq` | this parameter is required when the parameter in the map is equal to the value in the map | `arg: val3: required_if_eq=val1=2 val2=4` |
| `required_if_eq_all` | this parameter is required when all the parameters in the map are equal to the value in the map | `arg: val3: required_if_eq_all=val1=2 val2=4` |

It is also possible to span the description across multiple lines, on the condition that the header is repeated, or by using the special `+:` header. Descriptions will be concatenated automatically by omni:
```bash
# arg: -m, --min: tupe=int
# arg: -m, --min: Minimum value for the random number, this will
# arg: -m, --min: be used alongside the maximum value to return
# arg: -m, --min: a valid random number.
```

or:
```bash
# opt: -m, --min: type=int
# +: Minimum value for the random number, this will
# +: be used alongside the maximum value to return
# +: a valid random number.
```

This would show that the command has a `min` argument, and show its description in the help message.

If you wish to, you can also use shell coloring and formatting codes such as `\033` and `\x1B` in the description. It is recommended to avoid `\e` as it is not supported by older shells.

### `arggroup`

The `arggroup` header allows to define a group of parameters that, by default, are mutually exclusive.

This can be provided as follows:
```bash
# arggroup: group1: val1 val2
```

This would show that the command has a group of parameters `val1` and `val2`, and show its description in the help message. When using the argument parser, only one of the parameters in the group can be used at a time in this case.

It is possible to define options for the group allowing multiple values, or defining requirements for the whole group. The accepted parameters are the following:

| Parameter | Description | Example |
|-----------|-------------|---------|
| `required` | whether or not this group is required | `arggroup: group1: required=true: val1 val2` |
| `multiple` | whether or not multiple values can be provided for the group | `arggroup: group1: multiple=true: val1 val2` |
| `requires` | list of groups that are required when this group is present | `arggroup: group1: requires=val3 group2: val1 val2` |
| `conflicts_with` | list of groups that cannot be used with this group | `arggroup: group1: conflicts_with=val3 group2: val1 val2` |

### `help`

The `help` header allows to define the help message shown for the command. Note that you do not need to define the usage syntax as it will be automatically parsed from `arg` and `opt` headers. This is expected to be mostly a description of what the command does.

You will want to split the `help` header in two, as you would do for a git commit: the first paragraph, separed from the rest by a blank `help` header line, will be used as *short help* and shown directly in `omni help`. The whole content of the help will be used as *long help* and be shown when calling `omni help <command>`.

This can be provided as follows, with only a short help:
```bash
# help: Random number generator
```

Or with both short and long helps:
```bash
# help: Random number generator
# +:
# +: This will generate a random number using the pseudo-random
# +: generator of bash, as provided with the $RANDOM variable,
# +: from a minimum you define, to a maximum you can define or
# +: will default to twice the minimum.
```

If you wish to, you can also use shell coloring and formatting codes such as `\033` and `\x1B` in the description. It is recommended to avoid `\e` as it is not supported by older shells.

:::tip Delegating help to omni
When integrating a command with omni that takes a `-h` or `--help` parameter, you can simply have the command use omni to show its help message, which will allow for a unified view between `omni help <command>` and `omni <command> --help`.

For instance, using bash, you could write the following:
```bash
usage() {
        omni help ${OMNI_SUBCOMMAND}
        exit ${1:-0}
}
```

If you want this to work even if someone directly calls the script, not through omni, we could write the following, if our command is exposed as `omni rng`:
```bash
usage() {
        omni help ${OMNI_SUBCOMMAND:-rng}
        exit ${1:-0}
}
```

This would also work for any other language in which the command has been written.
:::

## Metadata file

The metadata file is in YAML format.

For a given path command, it will be located at the first existing readable file of:
- `<path_to_executable_file>/<executable_file_name_with_ext>.metadata.yaml`
- `<path_to_executable_file>/<executable_file_name>.metadata.yaml`

For instance, for a path command located at `/path/to/my/cmd/rng.sh`, omni will look for a metadata file at:
- `/path/to/my/cmd/rng.sh.metadata.yaml`
- `/path/to/my/cmd/rng.metadata.yaml`

### Parameters

The metadata file accepts the following parameters:

| Parameter               | Type | Description                                                            |
|-------------------------|------|------------------------------------------------------------------------|
| `autocompletion` | bool | whether or not the command supports autocompletion |
| `argparser` | bool | whether or not omni should parse arguments for the command |
| `category` | list | the category of the command; can be provided as an array or as a comma-separated list in string format. |
| `help` | string | the help of the command that will be used in `omni help`. This can be on multiple lines, in which case the first paragraph (until the first empty line) will be shown in `omni help`, while the rest of the help message will be shown when calling `omni help <command>`. |
| `syntax` | [`syntax`](/reference/configuration/parameters/commands#syntax) | Define the parameters that the command can take. This will be used when calling `omni help <command>`. |

### Example

```yaml
# Enable autocompletion handling through --complete
autocompletion: true

# Enable argument parsing
argparser: true

# Specify the command category
category:
  - General
  - Generators

# Specify the help message
help: |
  Random number generator

  This will generate a random number using the pseudo-random
  generator of bash, as provided with the $RANDOM variable,
  from a minimum you define, to a maximum you can define or
  will default to twice the minimum.

# Specify the command parameters
syntax:
  - name: min
    desc: Minimum value for the random number
    required: false
  - name: max
    desc: Maximum value for the random number
    required: false
```
