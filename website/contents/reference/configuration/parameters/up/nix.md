---
description: Configuration of the `nix` kind of `up` parameter
---

# `nix` operation

Installs nix packages and loads them into the environment.

Similarly to [`nix-direnv`](https://github.com/nix-community/nix-direnv), this prevents garbage collection of dependencies by symlinking the resulting shell derivation in the user's gcroots.

:::info
If `nix` is not available on the system, this step will be ignored.
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
  # Will look for `shell.nix`, `default.nix` or `flake.nix`
  # at the root of the work directory and use it to build
  # dependencies
  - nix

  # Will install the listed packages
  - nix:
    - gawk
    - gcc
    - gnused

  # Will install the packages defined in `file.nix`
  - nix: file.nix

  # Will install the packages defined in the flake `flake.nix`
  - nix: flake.nix
```

## Dynamic environment

The following variables will be set as part of the [dynamic environment](/reference/dynamic-environment).

| Environment variable | Operation | Description |
|----------------------|-----------|-------------|
| `CFLAGS` | suffix | The flags read from the `NIX_CFLAGS_COMPILE_FOR_TARGET` variable in the `nix` derivation |
| `CPPFLAGS` | suffix | The flags read from the `NIX_CFLAGS_COMPILE_FOR_TARGET` variable in the `nix` derivation |
| `LDFLAGS` | suffix | The flags read from the `NIX_LDFLAGS` variable in the `nix` derivation |
| `PATH` | prepend | The `bin` directories for all the packages, read from the `pkgsHostTarget` and `pkgsHostHost` variables in the `nix` derivation |
| `PKG_CONFIG_PATH` | append | The `pkgconfig` read from the `PKG_CONFIG_PATH_FOR_TARGET` variable in the `nix` derivation |
