---
description: Configuration of the `suggest_clone` parameter
---

# `suggest_clone`

:::info
This parameter can only be used inside of a git repository. Any global configuration for that parameter will be ignored.
:::

Repositories that a git repository suggests should be cloned, this is picked up when calling `omni up --clone-suggested` or when this command is directly called by `omni clone`.


## Parameters

Contains a list of objects with the following parameters:

| Parameter        | Type      | Description                                           |
|------------------|-----------|-------------------------------------------------------|
| `handle` | string | The repository handle, corresponding to the URL allowing to clone the repository |
| `args` | string | The optional arguments to pass to the `git clone` command |


## Examples

```yaml
# To suggest cloning the omni repository
suggest_clone:
  - git@github.com:XaF/omni

# To suggest cloning the omni repository, and the omni-example one
suggest_clone:
  - https://github.com/XaF/omni
  - handle: https://github.com/omnicli/omni-example

# If we want to suggest cloning the omni repository, but only with a depth of 1
suggest_clone:
  - handle: git@github.com:XaF/omni
    args: --depth 1
```
