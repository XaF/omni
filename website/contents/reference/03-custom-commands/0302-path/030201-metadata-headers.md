---
description: Metadata available for custom commands from path
---

# Metadata

Omni supports a number of metadata for custom commands provided through the omnipath, telling omni how to behave, or simply show help for that command.

Commands metadata can be provided either through a dedicated metadata file, colocated with the executable file providing the command, or through metadata headers added directly to the executable file, if not in binary format.

## Metadata headers

To be read properly, omni headers need to:
- Be right at the top of the file, in the header comment with only comment lines preceding it
- Not have lines that are not headers between headers
- Be before the `help` header (omni stops reading the file after the `help` header)

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

### `arg`

The `arg` header allows to define arguments that the command takes. These are not being parsed by omni, but will be shown when running `omni help <command>`. When using the `arg` header, you need to define the argument name or format, and the description/help for that argument.

This can be provided as follows:
```bash
# arg: min: Minimum value for the random number
```

It is also possible to span the description across multiple lines, on the condition that the header is repeated. Descriptions will be concatenated automatically by omni:
```bash
# arg: min: Minimum value for the random number, this will
# arg: min: be used alongside the maximum value to return
# arg: min: a valid random number.
```

This would show that the command has a `min` argument, and show its description in the help message.

If you wish to, you can also use shell coloring and formatting codes such as `\033` and `\x1B` in the description. It is recommended to avoid `\e` as it is not supported by older shells.

### `opt`

The `opt` header allows to define optional parameters that the command takes. These are not being parsed by omni, but will be shown when running `omni help <command>`. When using the `opt` header, you need to define the option name or format, and the description/help for that option.

This can be provided as follows:
```bash
# opt: max: Maximum value for the random number
```

It is also possible to span the description across multiple lines, on the condition that the header is repeated. Descriptions will be concatenated automatically by omni:
```bash
# opt: max: Maximum value for the random number, if not
# opt: max: provided, this will default to `min` * 2.
```

This would show that the command has a `max` option, and show its description in the help message.

If you wish to, you can also use shell coloring and formatting codes such as `\033` and `\x1B` in the description. It is recommended to avoid `\e` as it is not supported by older shells.

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
# help:
# help: This will generate a random number using the pseudo-random
# help: generator of bash, as provided with the $RANDOM variable,
# help: from a minimum you define, to a maximum you can define or
# help: will default to twice the minimum.
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
| `autocompletion` | bool | whether or not the command supports autocompletion
| `category` | list | the category of the command; can be provided as an array or as a comma-separated list in string format. |
| `help` | string | the help of the command that will be used in `omni help`. This can be on multiple lines, in which case the first paragraph (until the first empty line) will be shown in `omni help`, while the rest of the help message will be shown when calling `omni help <command>`. |
| `syntax` | [`syntax`](/reference/configuration/parameters/commands#syntax) | Define the parameters that the command can take. This will be used when calling `omni help <command>`. |

### Example

```yaml
# Enable autocompletion handling through --complete
autocompletion: true

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
