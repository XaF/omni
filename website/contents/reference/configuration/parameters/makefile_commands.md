---
description: Configuration of the `makefile_commands` parameter
---

# `makefile_commands`

## Parameters

Configuration related to the commands generated from Makefile targets.

| Parameter        | Type      | Description                                           |
|------------------|-----------|-------------------------------------------------------|
| `enabled` | boolean |  whether or not to load commands from the Makefiles in the current path and parents (up to the root of the git repository, or user directory) *(default: true)* |
| `split_on_dash` | boolean | whether or not the targets should be split on dash (e.g. 'my-target' would be used as 'omni my target' instead of 'omni my-target') *(default: true)* |
| `split_on_slash` | boolean | whether or not the targets should be split on slash (e.g. 'my/target' would be used as 'omni my target' instead of 'omni my/target') *(default: true)* |

## Example

```yaml
makefile_commands:
  enabled: true
  split_on_dash: true
  split_on_slash: true
```
