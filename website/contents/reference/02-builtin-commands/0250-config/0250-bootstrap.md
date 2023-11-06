---
description: Builtin command `config bootstrap`
---

# `bootstrap`

Bootstraps the configuration of omni

This will walk you through setting up the initial configuration to use omni, such as setting up the [worktree](../../configuration/parameters/worktree), [format to use when cloning repositories](../../configuration/parameters/repo_path_format), and setting up initial [organizations](../../configuration/parameters/org).

This command will be triggered automatically if no user-level configuration file is detected.

This command should be safe to call even with an existing configuration file. It will however override the specific configuration parameters with the new values set during the guided process.

## Examples

```bash
# Starts the bootstrap process
omni config bootstrap
```
