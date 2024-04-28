---
description: Builtin command `hook`
---

# `hook`

Call one of omni's hooks for the shell.

## `init`

The `init` hook will provide you with the command to run to initialize omni in your shell.

### Parameters

| Parameter       | Required | Value type | Description                                         |
|-----------------|----------|------------|-----------------------------------------------------|
| `shell` | no | enum: `zsh`, `bash` or `fish` | The shell for which to provide the shell integration; defaults to the value of `SHELL` environment variable, or `bash` otherwise. |
| `--alias <alias>` | no | string | Adds `<alias>` as a shell alias to the `omni` command, with autocompletion support; can be repeated. |
| `--command-alias <alias> <subcommand>` | no | string, string | Adds `<alias>` as a shell alias to the `omni <subcommand>` command, with autocompletion support; can be repeated. |
| `--shims` | no | `null` | Only load the shims without setting up the dynamic environment. |
| `--keep-shims` | no | `null` | Keep the shims in the path, instead of removing them when setting up the dynamic environment. |
| `--print-shims-path` | no | `null` | Print the path to the shims directory and exit immediately. Will not initialize omni's environment for the current shell. |

### Examples

```bash
# While specifying the shell
eval "$(omni hook init bash)"    # for bash
eval "$(omni hook init zsh)"     # for zsh
omni hook init fish | source     # for fish

# If not specifying the shell, the login shell, as reflected by the `SHELL`
# environment variable, is used
eval "$(omni hook init)"
omni hook init | source

# With 'o' as an omni alias
eval "$(omni hook init --alias o)"
omni hook init --alias o | source

# With 'ocd' as an alias to 'omni cd'
eval "$(omni hook init --command-alias ocd cd)"
omni hook init --command-alias ocd cd | source

# Only load the shims
eval "$(omni hook init bash --shims)"  # for bash
eval "$(omni hook init zsh --shims)"   # for zsh
omni hook init fish --shims | source   # for fish
```

## `env`

The `env` hook is called during your shell prompt or before executing a shim to set the [dynamic environment](/reference/dynamic-environment) for `omni up`-ed repositories.

## `uuid`

The `uuid` hook provides and alternative to `uuidgen`, in case it is not installed, so that omni can work without extra dependencies.
