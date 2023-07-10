---
description: Configuration of the `worktree` parameter
---

# `worktree`

## Parameters

Location of the default worktree.

If this value is not set, it will default to, in order:
- `~/git` if the path exists
- `$GOPATH/src` if `$GOPATH` is defined and the path exists
- `~/git`

The default value will be ignored if `OMNI_GIT` is defined, as it overrides the worktree location.

## Examples

```yaml
# Using an absolute path
worktree: /absolute/path/to/my/worktree

# Using a relative path - this is relative to the location of
# the configuration file containing that configuration entry
worktree: relative/path/to/my/worktree

# Using a home-prefixed path
worktree: ~/worktree
```

## Environment

The environment variable `OMNI_GIT` overrides the worktree location on the condition that it is targetting an absolute path. If the variable is empty or contains a relative path, it will be ignored.

```bash
# Overrides worktree
export OMNI_GIT=/absolute/path/to/my/worktree
# Ignored, worktree configuration will be used
export OMNI_GIT=relative/path/to/my/worktree
# Ignored, worktree configuration will be used
export OMNI_GIT=
```
