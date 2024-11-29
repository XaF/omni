---
description: Configuration of the `cargo-install` kind of `up` parameter
---

# `cargo-install` operation

Install a tool through `cargo install`.

This is done in a way that is shareable across work directories managed by omni; i.e. once a tool is installed in a given version, if required from another work directory it will not need to be reinstalled.

This will automatically install a version of [`rust`](rust) if none is available through omni to run the `cargo install` command, but won't add it to the dynamic environment.

## Parameters

| Parameter        | Type      | Description                                           |
|------------------|-----------|-------------------------------------------------------|
| `crate` | string | The name of the crate to install |
| `version` | string | The version to install; see [version handling](#version-handling) below for more details. |
| `exact` | boolean | Whether to match the exact version or not; if set to `true`, `cargo install <crate>@<version>` will be called directly instead of listing the available versions and following the [version handling](#version-handling) rules *(default: `false`)* |
| `upgrade` | boolean | whether or not to always upgrade to the most up to date matching release, even if an already-installed version matches the requirements *(default: false)* |
| `prerelease` | boolean | Whether to download a prerelease version or only match stable releases; this will also apply to versions with prerelease specification, e.g. `1.2.3-alpha`. Ignored when `exact` is set to `true` *(default: `false`)* |
| `build` | boolean | Whether to download a version with build specification, e.g. `1.2.3+build`. Ignored when `exact` is set to `true` *(default: `false`)* |

### Version handling

The following strings can be used to specify the version:

| Version | Meaning |
|---------|---------|
| `1.2`     | Accepts `1.2` and any version prefixed by `1.2.*` |
| `1.2.3`   | Accepts `1.2.3` and any version prefixed by `1.2.3.*` |
| `~1.2.3`  | Accepts `1.2.3` and higher patch versions (`1.2.4`, `1.2.5`, etc. but not `1.3.0`) |
| `^1.2.3`  | Accepts `1.2.3` and higher minor and patch versions (`1.2.4`, `1.3.1`, `1.4.7`, etc. but not `2.0.0`) |
| `>1.2.3`  | Must be greater than `1.2.3` |
| `>=1.2.3` | Must be greater or equal to `1.2.3` |
| `<1.2.3`  | Must be lower than `1.2.3` |
| `<=1.2.3` | Must be lower or equal to `1.2.3` |
| `1.2.x`   | Accepts `1.2.0`, `1.2.1`, etc. but will not accept `1.3.0` |
| `*`       | Matches any version (same as `latest`, except that when `upgrade` is `false`, will match any installed version) |
| `latest`  | Latest release (when `upgrade` is set to `false`, will only match with installed versions of the latest major) |

The version also supports the `||` operator to specify ranges. This operator is not compatible with the `latest` and keywords. For instance, `1.2.x || >1.3.5 <=1.4.0` will match any version between `1.2.0` included and `1.3.0` excluded, or between `1.3.5` excluded and `1.4.0` included.

The latest version satisfying the requirements will be installed.

## Examples

```yaml
up:
  # Will error out since no repository is provided
  - cargo-install

  # Will install the latest release of `ripgrep`
  - cargo-install: ripgrep

  # Will also install the latest version
  - cargo-install:
      crate: ripgrep
      version: latest

  # Will install any version starting with 14.0
  - cargo-install:
      crate: ripgrep
      version: 14.0

  # Will install any version starting with 14
  - cargo-install:
      crate: ripgrep
      version: 14

  # Full specification of the parameter to identify the version;
  # this will install any version starting with 14.0.1
  - cargo-install:
      crate: ripgrep
      version: 14.0.1

  # Will install any version starting with 14, including
  # any pre-release versions
  - cargo-install:
      path: ripgrep
      version: 14
      prerelease: true

  # Will install all the specified releases
  - cargo-install:
      ripgrep: 14.0.1
      exa:
        version: 0.9.0
        build: true

  # Will install all the listed releases
  - cargo-install:
      - ripgrep@14.0.1
      - exa: 0.9.0
      - crate: bat
        version: 0.15.0
```

## Dynamic environment

The following variables will be set as part of the [dynamic environment](/reference/dynamic-environment).

| Environment variable | Operation | Description |
|----------------------|-----------|-------------|
| `PATH` | prepend | Injects the path to the binaries of the installed tool |
