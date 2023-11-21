---
description: Builtin command `config bootstrap`
---

# `bootstrap`

Bootstraps the configuration of omni

This will walk you through setting up the initial configuration to use omni, such as setting up the [worktree](../../configuration/parameters/worktree), [format to use when cloning repositories](../../configuration/parameters/repo_path_format), and setting up initial [organizations](../../configuration/parameters/org).

This command will be triggered automatically if no user-level configuration file is detected.

This command should be safe to call even with an existing configuration file. It will however override the specific configuration parameters with the new values set during the guided process.

## Parameters

| Parameter       | Required | Value type | Description                                         |
|-----------------|----------|------------|-----------------------------------------------------|
| `--worktree` | no | `null` | Bootstrap the main worktree location. If specified, only this and other specified bootstraps will be performed. |
| `--repo-path-format` | no | `null` | Bootstrap the repository path format. If specified, only this and other specified bootstraps will be performed. |
| `--organizations` | no | `null` | Bootstrap the organizations. If specified, only this and other specified bootstraps will be performed. |
| `--shell` | no | `null` | Bootstrap the shell integration. If specified, only this and other specified bootstraps will be performed. |

## Examples

```bash
# Starts the bootstrap process
omni config bootstrap

# Only bootstrap the worktree
omni config bootstrap --worktree

# Bootstrap the worktree and organizations
omni config bootstrap --worktree --organizations

# Bootstrap the shell integration
omni config bootstrap --shell
```
