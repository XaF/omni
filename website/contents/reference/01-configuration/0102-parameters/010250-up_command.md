---
description: Configuration of the `up_command` parameter
---

# `up_command`

Configuration related to the `omni up` command.

## Parameters

| Parameter       | Type      | Description                                         |
|-----------------|-----------|-----------------------------------------------------|
| `auto_bootstrap` | boolean | whether or not to automatically infer the `--bootstrap` parameter when running `omni up`, if changes to the configuration suggestions from the work directory are detected *(default: true)* |
| `notify_workdir_config_updated` | boolean | whether or not to print a message on the prompt if the `up` configuration of the work directory has been updated since the last `omni up` *(default: true)* |
| `notify_workdir_config_available` | boolean | whether or not to print a message on the prompt if the current work directory has an available `up` configuration but `omni up` has not been run yet *(default: true)* |
| `preferred_tools` | list | list of preferred tools for [`any` operations](up/any) when running `omni up`; those tools will be preferred over others, in the order they are defined |
| `mise_version` | string | the version of [`mise`](https://mise.jdx.dev/) to use for the installation of tools that depend on it *(default: `latest`)* |
| `upgrade` | boolean | whether or not to always upgrade to the most up to date matching version of the dependencies when running `omni up`, even if an already-installed version matches the requirements *(default: false)* |
| `operations` | `Operations` object | configuration of the `up` operations, with a number of settings oriented toward supply-chain management and security |

### `Operations` object

| Parameter       | Type      | Description                                         |
|-----------------|-----------|-----------------------------------------------------|
| `allowed` | list | list of allowed operations (e.g. `go-install`, `github-release`, etc.) to be run when executing `omni up`. If empty, all operations are allowed. Entries in the list prefixed by `!` are disallowed, and wildcards are allowed. Entries are processed in order, so the first match (either allowed or disallowed) is used. For operations using `mise` as backend, only the `mise` operation name will be matched on. *(default: empty)* |
| `sources` | list | list of allowed sources (urls) for the operations to be run when executing `omni up`. If empty, all sources are allowed. Entries in the list prefixed by `!` are disallowed, and wildcards are allowed. Entries are processed in order, so the first match (either allowed or disallowed) is used. This parameter applies over all operations using sources (e.g. `go-install`, `github-releases`, `mise` plugins, etc.) *(default: empty)* |
| `mise` | `Mise` object | configuration of the `mise` operations, i.e. of all the operations that use `mise` as backend |
| `cargo-install` | `CargoInstall` object | configuration of the `cargo-install` operations |
| `go-install` | `GoInstall` object | configuration of the `go-install` operations |
| `github-release` | `GithubRelease` object | configuration of the `github-release` operations |

::: tip
When using the allow/deny lists, the last entry will indicate the default behavior for all values that are not matched by any of the previous entries.

For example, if the list is `['!a*', 'b*']`, all values starting with `a` will be denied, all values starting with `b` will be allowed, and all other values will be denied (opposite of the last entry, which is an allow).

In contrast, if the list is `['b*', '!a*']`, all values starting with `b` will be allowed, and all values starting with `a` will be denied, then all other values will be allowed (opposite of the last entry, which is a deny).
:::

#### `Mise` object

| Parameter       | Type      | Description                                         |
|-----------------|-----------|-----------------------------------------------------|
| `backends` | list | list of allowed backends (e.g. `core`, `aqua`, `vfox`, `asdf`, etc.) for the `mise` operations. If empty, all backends are allowed. Entries in the list prefixed by `!` are disallowed, and wildcards are allowed. Entries are processed in order, so the first match (either allowed or disallowed) is used. The special `custom` backend can be used to represent any plugin installed from a provided URL. *(default: empty)* |
| `sources` | list | same as `sources` in the `Operations` object, but applies only to `mise` operations *(default: empty)* |
| `default_plugin_sources` | map | map of default sources for the `mise` operations, where the key is the tool name (e.g. `python`) and the value is the source URL (e.g. `https://github.com/asdf-community/asdf-python`). This is used when no source is provided in the configuration of the operation, and overrides any default URL that would be read from the `mise` registry. *(default: empty)* |

#### `CargoInstall` object

| Parameter       | Type      | Description                                         |
|-----------------|-----------|-----------------------------------------------------|
| `crates` | list | list of allowed crates for the `cargo-install` operations. If empty, all crates are allowed. Entries in the list prefixed by `!` are disallowed, and wildcards are allowed. Entries are processed in order, so the first match (either allowed or disallowed) is used. *(default: empty)* |

#### `GoInstall` object

| Parameter       | Type      | Description                                         |
|-----------------|-----------|-----------------------------------------------------|
| `sources` | list | same as `sources` in the `Operations` object, but applies only to `go-install` operations *(default: empty)* |

#### `GithubRelease` object

| Parameter       | Type      | Description                                         |
|-----------------|-----------|-----------------------------------------------------|
| `repositories` | list | list of allowed repositories in the `<owner>/<repo>` format for the `github-release` operations. If empty, all repositories are allowed. Entries in the list prefixed by `!` are disallowed, and wildcards are allowed. Entries are processed in order, so the first match (either allowed or disallowed) is used. *(default: empty)* |

## Example

```yaml
up_command:
  # Whether or not to automatically infer the `--bootstrap` parameter when running `omni up`
  auto_bootstrap: true

  # Whether or not to notify the user about the workdir configuration
  notify_workdir_config_updated: true

  # Whether or not to notify the user about the available workdir configuration
  notify_workdir_config_available: true

  # List of preferred tools for `any` operations when running `omni up`
  preferred_tools:
  - nix
  - brew
  - apt

  # The version of `mise` to use for the installation of tools that depend on it
  mise_version: latest

  # Whether or not to always upgrade to the most up to date matching
  # version of the dependencies when running `omni up`
  upgrade: false

  # Configuration of the up operations
  operations:
    # List of allowed/denied operations
    allowed:
      - go-install
      - mise
      - '!cargo-install'
      - '!github-release'
      - '!nix'

    # Global URL allow/deny list
    sources:
      - 'github.com/trusted-org/*'
      - 'gitlab.com/trusted-org/*'
      - '!github.com/bad-owner/*'
      - '!*.suspicious-domain.com'

    # Operation-specific configuration
    mise:
      backends:
        - asdf
        - aqua
        - '!a*'  # Disallow all backends starting with 'a', allow all others
      sources:
        - 'github.com/additional-trusted-org/trusted-*'
        - '!github.com/additional-trusted-org/*'
      default_plugin_sources:
        python: 'https://github.com/custom-org/asdf-python.git'
        node: 'https://github.com/custom-org/asdf-node.git'

    cargo-install:
      crates:
        - ripgrep  # Allow ripgrep, deny all others

    go-install:
      sources:
        - 'golang.org/*'
        - '!github.com/method-specific-blocked-org/*'

    github-release:
      repositories:
        - 'owner/*'
        - '!owner/legacy-*'
```
