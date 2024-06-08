---
description: Configuration of the `askpass` parameter
---

# `askpass`

## Parameters

Configuration related to the handling of `*_ASKPASS` environment variables when doing omni operations that might require a password input.

At this time, only `SSH_ASKPASS` and `SUDO_ASKPASS` are supported.

| Parameter        | Type      | Description                                           |
|------------------|-----------|-------------------------------------------------------|
| `enabled` | boolean | whether or not omni should try handling askpass environment variables if unset *(default: true)* |
| `enable_gui` | boolean | whether or not omni should enable using a gui tooling to ask for password if available *(default: true)* |
| `prefer_gui` | boolean | whether or not a gui tooling to ask for password should be preferred if available (only supported on MacOS for now) *(default: false)* |

## Example

```yaml
askpass:
  enabled: true
  enable_gui: true
  prefer_gui: false
```
