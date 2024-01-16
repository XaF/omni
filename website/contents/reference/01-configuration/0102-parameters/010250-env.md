---
description: Configuration of the `env` parameter
---

# `env`

## Parameters

Contains a list of the environment variables to be set, and their values.

For each environment variables, these are the accepted parameters:

| Parameter       | Type      | Description                                         |
|-----------------|-----------|-----------------------------------------------------|
| `value` | string | The value to set for the environment variable; if set to `null`, the environment variable will be unset |
| `type` | enum | One of `text` for a static value, or `path` for the value to be converted into an absolute path *(default: text)* |

Special blocks are supported for operations on lists. The `append` block will append the proposed value to the list, `prepend` will prepend it, and `remove` will remove it from the list. The `set` block is the one used by default, and simply sets the value of the environment variable.

## Example

```yaml
# Simple setting of variables
env:
  VAR1: VAL1
  VAR2: VAL2

# Doing list operations; this will prepend the relative path path/to/my/lib
# directory to the PYTHONPATH environment variable. Using `type: path`, this
# will be converted into an absolute variable, taking as current directory
# the configuration file in which that variable is.
env:
  PYTHONPATH:
    prepend:
      value: path/to/my/lib
      type: path

# It is possible to specify multiple modifiers at the same time, and multiple
# values for the same modifier by passing a list
env:
  VAR1:
    prepend:
      - val1
      - val2
    append: val3

# When passed as a list, allows for the same variable to be specified twice
env:
  - VAR1: VAL1
  - VAR2: VAL2
```
