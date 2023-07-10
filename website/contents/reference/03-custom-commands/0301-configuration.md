---
description: Custom commands from Configuration
---

# Configuration commands

You can check [the `commands` configuration parameter](/reference/configuration/parameters/commands) to read how to define configuration commands.

:::info Current working directory
Configuration commands are run from the directory in which the configuration file defining them is located to make sure that any relative path used in the command will always be valid.
:::

:::tip Scope
Configuration commands are scoped to a repository when defined in the omni configuration of that repository, or can be made available everywhere if defined in a global configuration file. If you want to access a repository-scoped configuration command, you can use [`omni scope`](/reference/builtin-commands/scope).
:::

## Environment

The following environment variables are set by omni before the Configuration command is called:

| Environment variable | Type | Description |
|----------------------|------|-------------|
| `OMNI_SUBCOMMAND` | string... | The subcommand that was called leading to the execution of that command; e.g. `my command` for `omni my command` |
| `OMNI_CWD` | path | The current working directory where `omni` was called from |

The following environment variables are set by the shell integration and can be taken advantage of when writing commands:

| Environment variable | Type | Description |
|----------------------|------|-------------|
| `OMNI_SHELL` | string | The shell of the user for which the shell integration was loaded |
| `OMNI_CMD_FILE` | filepath | The file in which omni will read operations to apply to the shell; this needs to be compatible with the shell of the user as provided by `OMNI_SHELL` |
