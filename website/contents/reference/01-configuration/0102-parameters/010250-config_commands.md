---
description: Configuration of the `config_commands` parameter
---

# `config_commands`

## Parameters

Configuration related to the commands defined in the config file.

| Parameter        | Type      | Description                                           |
|------------------|-----------|-------------------------------------------------------|
| `split_on_dash` | boolean | whether or not the commands should be split on dash (e.g. 'my-command' would be used as 'omni my command' instead of 'omni my-command') *(default: true)* |
| `split_on_slash` | boolean | whether or not the commands should be split on slash (e.g. 'my/command' would be used as 'omni my command' instead of 'omni my/command') *(default: true)* |

## Example

```yaml
config_commands:
  split_on_dash: true
  split_on_slash: true
```
