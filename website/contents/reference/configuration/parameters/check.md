---
description: Configuration of the `check` parameter
---

# `check`

Configuration for the [`config check`](/reference/builtin-commands/config/check) command.

## Parameters

This is expected to be a list of objects containing the following parameters:

| Parameter       | Type      | Description                                         |
|-----------------|-----------|-----------------------------------------------------|
| `patterns` | list of strings | Pattern of files to include (or exclude, if starting by `!`) in the check. Allows for glob patterns to be used. |
| `ignore` | list of strings | [Error codes](/reference/builtin-commands/config/check#error-codes) to ignore. |
| `select` | list of strings | [Error codes](/reference/builtin-commands/config/check#error-codes) to select. |

## Example

```yaml
check:
  patterns:
    # Match yaml files at the root of the repository
    - "*.yaml"
    # Match yaml files in all subdirectories
    - "**/*.yaml"
    # Exclude yaml files in the `test` directory
    - "!test/**/*.yaml"
    # Exclude all files in the `test` directory and its subdirectories
    - "!test/**"

  ignore:
    - "C"    # Ignore all errors of type C
    - "M0"   # Ignore all errors of type M0
    - "P002" # Ignore all errors of type P002

  select:
    - "C001" # Select all errors of type C001
    - "M"    # Select all errors of type M
    - "P0"   # Select all errors of type P
```
