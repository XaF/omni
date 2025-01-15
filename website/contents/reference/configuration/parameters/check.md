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
| `tags` | list of strings or objects | Tags to include in the check, and how to validate them. The elements of the list can be a string, in which case it is assumed to be a tag name to require, or a key-value pair where the value is a [Filter](github#filter-object) object. |

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

  tags:
    - should exist
    - should match: 'val*'
    - should exactly match:
        exact: 'value'
    - should be a number:
        regex: '^\d+$'
```
