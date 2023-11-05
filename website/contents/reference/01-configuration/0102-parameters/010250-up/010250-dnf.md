---
description: Configuration of the `dnf` kind of `up` parameter
sidebar_label: dnf operation ⚠
---

# `dnf` operation

:::caution
This configuration hasn't been ported from the ruby version of `omni` yet.
It will eventually be supported again, but is not for now.
You can comment on [this issue](https://github.com/XaF/omni/issues/202) to manifest your interest.
:::

Installs dnf packages.

:::info
If `dnf` is not available on the system, this step will be ignored.
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
  - dnf

  # Will install the default version of the package
  - dnf:
    - make

  # Will also install the default version of the package
  - dnf:
    - package: make

  - dnf:
    # Can specify another version
    - package: gparted
      version: 0.16.1-1

  # This syntax also works to install a specific version
  - dnf:
    - gparted: 0.16.1-1
```
