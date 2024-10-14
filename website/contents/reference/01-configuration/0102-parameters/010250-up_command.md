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
| `upgrade` | boolean | whether or not to always upgrade to the most up to date matching version of the dependencies when running `omni up`, even if an already-installed version matches the requirements *(default: false)* |

## Example

```yaml
up_command:
  auto_bootstrap: true
  notify_workdir_config_updated: true
  notify_workdir_config_available: true
  preferred_tools:
  - nix
  - brew
  - apt
```
