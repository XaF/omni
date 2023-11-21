---
description: Builtin command `config path switch`
---

# `switch`

Switch the source of a repository in the omnipath.

This allows to change the omnipath source from using a package or a development version in a worktree.

When switching into a mode, if the source of the requested type does not exist, the repository will be cloned. This allows to directly get a worktree version of a given package, or easily package a repository.

Upon switching to a package, `omni up --update-repository` will automatically be run to make sure the package is up to date. This will not happen when switching to a development version in a worktree.

This command requires the target to be a repository, and for the repository to already be cloned, either in a worktree or as a package.

## Parameters

| Parameter       | Required | Value type | Description                                         |
|-----------------|----------|------------|-----------------------------------------------------|
| `--package` | no | `null` | Switches the omnipath to the package version of the repository. Cannot be used alongside `--worktree`. If not specified and `--worktree` is not specified either, will default to toggle the current state. |
| `--worktree` | no | `null` | Switches the omnipath to the development version of the repository in a worktree. Cannot be used alongside `--package`. If not specified and `--package` is not specified either, will default to toggle the current state. |
| `repo` | no | string | The name of the repository to switch the source from; this can be in the format `<org>/<repo>`, or just `<repo>`. If the repository is not provided, the current repository will be used, or the command will fail if not in a repository. If the repo is not found in the omnipath, the command will fail. |

## Examples

```bash
# Toggles the source of the current repository in the omnipath
omni config path switch

# Toggles the source of the omni repository in the omnipath
omni config path switch omni

# Toggles the source of the omni repository in the omnipath
omni config path switch https://github.com/XaF/omni

# Switches the source of the current repository to package
omni config path switch --package

# Switches the source of the current repository to a development
# version in a worktree
omni config path switch --worktree

# Switches the source of the omni repository to package
omni config path switch --package omni

# Switches the source of the omni repository to a development
# version in a worktree
omni config path switch --worktree https://github.com/XaF/omni
```
