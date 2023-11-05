---
description: Configuration of the `pacman` kind of `up` parameter
sidebar_label: pacman operation ⚠
---

# `pacman` operation

:::caution
This configuration hasn't been ported from the ruby version of `omni` yet.
It will eventually be supported again, but is not for now.
You can comment on [this issue](https://github.com/XaF/omni/issues/203) to manifest your interest.
:::

Installs pacman packages.

:::info
If `pacman` is not available on the system, this step will be ignored.
:::

## Parameters

Contains a list of objects with the following parameters:

| Parameter        | Type      | Description                                           |
|------------------|-----------|-------------------------------------------------------|
| `package` | string | The name of the package to install |
| `version` | string | The version to install for the package |

## Examples

```yaml
up:
  # Will do nothing if no parameters are passed
  - pacman

  # Will install the default version of the package
  - pacman:
    - make

  # Will also install the default version of the package
  - pacman:
    - package: make

  - pacman:
    # Can specify another version
    - package: gparted
      version: 0.16.1-1

  # This syntax also works to install a specific version
  - pacman:
    - gparted: 0.16.1-1
```
