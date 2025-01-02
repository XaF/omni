---
description: Builtin command `help`
---

# `help`

Show help for omni commands

If no command is given, show a list of all available commands.

:::info
Printing the help of a specific command will show you the `Source:` of that command. That can be practical if you're trying to track where is located the command being run when calling it through `omni`.
:::

## Parameters

| Parameter       | Required | Value type | Description                                         |
|-----------------|----------|------------|-----------------------------------------------------|
| `--unfold` | no | `null` | Show all the commands, instead of folding them |
| `command` | no | string... | The command to get help for. If not provided, will list all available commands. |

## Examples

```bash
# Show all available commands (folds when >1 subcommand)
omni help

# Show all available commands (no fold)
omni help --unfold

# Show help for a specific command, and list all subcommands (folds)
omni help cd

# Show help for a specific command, and list all subcommands (no fold)
omni help --unfold cd
```
