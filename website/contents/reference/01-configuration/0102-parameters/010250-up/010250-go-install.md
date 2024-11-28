---
description: Configuration of the `go-install` kind of `up` parameter
---

# `go-install` operation

Install a tool through `go install`.

This is done in a way that is shareable across work directories managed by omni; i.e. once a tool is installed in a given version, if required from another work directory it will not need to be reinstalled.

This will automatically install a version of `go` if none is available through omni to run the `go install` command.

## Parameters

| Parameter        | Type      | Description                                           |
|------------------|-----------|-------------------------------------------------------|
| `path` | string | The `<path>` part of `go install <path>[@<version>]` |
| `version` | string | The version to install; see [version handling](#version-handling) below for more details. |
| `exact` | boolean | Whether to match the exact version or not; if set to `true`, `go install <path>@<version>` will be called directly instead of listing the available versions and following the [version handling](#version-handling) rules *(default: `false`)* |
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
| `auto`    | Lookup for any version files in the project directory (`.tool-versions`, `.go-version`, `.golang-version` or `.go.mod`) and apply version parsing |

The version also supports the `||` operator to specify ranges. This operator is not compatible with the `latest` and `auto` keywords. For instance, `1.2.x || >1.3.5 <=1.4.0` will match any version between `1.2.0` included and `1.3.0` excluded, or between `1.3.5` excluded and `1.4.0` included.

The latest version satisfying the requirements will be installed.

## Examples

```yaml
up:
  # Will error out since no repository is provided
  - go-install

  # Will install the latest release of `protoc-gen-go`
  - go-install: google.golang.org/protobuf/cmd/protoc-gen-go

  # Will also install the latest version
  - go-install:
      path: google.golang.org/protobuf/cmd/protoc-gen-go
      version: latest

  # Will install any version starting with 1.27
  - go-install:
      path: google.golang.org/protobuf/cmd/protoc-gen-go
      version: 1.27

  # Will install any version starting with 1
  - go-install:
      path: google.golang.org/protobuf/cmd/protoc-gen-go
      version: 1

  # Full specification of the parameter to identify the version;
  # this will install any version starting with 1.27.0
  - go-install:
      path: google.golang.org/protobuf/cmd/protoc-gen-go
      version: 1.27.0

  # Will install any version starting with 1, including
  # any pre-release versions
  - go-install:
      path: google.golang.org/protobuf/cmd/protoc-gen-go
      version: 1
      prerelease: true

  # Will install all the specified releases
  - go-install:
      google.golang.org/protobuf/cmd/protoc-gen-go: 1.27.0
      google.golang.org/grpc/cmd/protoc-gen-go-grpc:
        version: 1.5.0
        prerelease: true

  # Will install all the listed releases
  - go-install:
      - google.golang.org/protobuf/cmd/protoc-gen-go@1.27.0
      - google.golang.org/grpc/cmd/protoc-gen-go: 1.27.1
      - path: google.golang.org/grpc/cmd/protoc-gen-go-grpc
        version: 1.5.0
```

## Dynamic environment

The following variables will be set as part of the [dynamic environment](/reference/dynamic-environment).

| Environment variable | Operation | Description |
|----------------------|-----------|-------------|
| `PATH` | prepend | Injects the path to the binaries of the installed tool |
