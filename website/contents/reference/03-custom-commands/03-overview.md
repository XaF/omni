---
description: Custom commands
slug: /reference/custom-commands
---

# Custom commands

Omni supports custom commands provided through different means:
- [Omni configuration files](custom-commands/configuration)
- [Paths added to your omnipath](custom-commands/path)
- [`Makefile` files in your git repository](custom-commands/makefile)

## Checking that a command exists

Some processes might require to verify that an omni command is available. This is possible through the use of the `--exists` global flag for any call to omni. When using this flag followed by a command, as you would call it through omni, the exit code will indicate if the command is being provided through omni (`0`) or does not seem to exist (`1`).

```bash
# Exits with 0 if the command exists
omni --exists help

# Exits with 1 if the command does not exist
omni --exists this is not a command

# Also works with the short flag
omni -e help
```
