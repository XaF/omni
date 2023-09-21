---
description: Configuration of the `up_command` parameter
---

# `up_command`

## Parameters

Configuration related to the `omni up` command.

| Parameter       | Type      | Description                                         |
|-----------------|-----------|-----------------------------------------------------|
| `auto_bootstrap` | boolean | whether or not to automatically infer the `--bootstrap` parameter when running `omni up`, if changes to the configuration suggestions from the work directory are detected |

## Example

```yaml
up_command:
  auto_bootstrap: true
```
