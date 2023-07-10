---
description: Builtin command `scope`
---

# `scope`

Runs an omni command in the context of the specified repository

This allows to run any omni command that would be available while in the repository directory, but without having to
change directory to the repository first.

## Parameters

### Arguments

| Argument        | Value type | Description                                         |
|-----------------|------------|-----------------------------------------------------|
| `repo` | string | The repository to clone; this can be in format `<org>/<repo>`, just `<repo>`, or the full URL. If the case where a full URL is not specified, the configured organizations will be used to search for the repository to clone. |
| `command` | string... | The omni command to run in the context of the specified repository. |

### Options

| Option          | Value type | Description                                         |
|-----------------|------------|-----------------------------------------------------|
| `options...` | any | Any options to pass to the omni command. |

## Examples

```bash
# The same kind of `repo` argument as provided to `omni cd` will work here
omni scope https://github.com/XaF/omni help
omni scope XaF/omni help
omni scope omni help
omni scope ~ help
omni scope /absolute/path help
omni scope relative/path help
omni scope .. help
```
