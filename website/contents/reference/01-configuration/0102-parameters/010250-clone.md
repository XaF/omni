---
description: Configuration of the `clone` parameter
---

# `clone`

## Parameters

Configuration related to the `omni clone` command.

| Parameter       | Type      | Description                                         |
|-----------------|-----------|-----------------------------------------------------|
| `auto_up` | boolean | Whether or not `omni up` should be run automatically after cloning a repository. *(default: `true`)* |
| `ls_remote_timeout` | duration | The duration after which to timeout when trying to find the remote of a repository during `omni clone` *(default: `5s`)* |

## Example

```yaml
clone:
  auto_up: true
  ls_remote_timeout: 5s
```
