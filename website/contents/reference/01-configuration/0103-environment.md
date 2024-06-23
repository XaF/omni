---
description: Environment variables that can be used with omni
---

# Environment variables

## Writeable

Omni supports a number of environment variables for its configuration. Setting those will generally override equivalent configuration options or precede them.

| Variable                | Type | Description                                                            |
|-------------------------|------|------------------------------------------------------------------------|
| `OMNIPATH` | colon-delimited list of paths | Provides the paths to different omni commands. See [parameters/path](parameters/path#environment) for more details. |
| `OMNI_CONFIG` | `filepath` | The path to an omni global configuration file. See [files](files#global-configuration). |
| `OMNI_FORCE_UPDATE` | `string` | Force-triggers omnipath and self updates when set to anything but an empty string, even if it should have triggered. It is recommended to either set to `1` or empty/unset. Is superseded by `OMNI_SKIP_UPDATE` and `OMNI_SKIP_SELF_UPDATE`. |
| `OMNI_GIT` | `path` | The worktree where omni will clone and look for repositories. Overrides the configuration. See [parameters/worktree](parameters/worktree#environment) for more details. |
| `OMNI_NONINTERACTIVE` | `string` | Disables interactive prompts when set to anything but an empty string. It is recommended to either set to `1` or empty/unset. |
| `OMNI_ORG` | comma-delimited list of strings | Prepend organizations to be considered by omni. e.g.: `OMNI_ORG="git@github.com:XaF,github.com/XaF"`. See [parameters/org](parameters/org#environment) for more details. |
| `OMNI_SKIP_SELF_UPDATE` | `string` | Disables self updates when set to anything but an empty string, even if it should have triggered. It is recommended to either set to `1` or empty/unset. |
| `OMNI_SKIP_UPDATE` | `string` | Disables omnipath and self updates when set to anything but an empty string, even if it should have triggered. It is recommended to either set to `1` or empty/unset. |

## Read-only

| Variable                | Type | Description                                                            |
| `OMNI_LOCAL_LOOKUP` | `boolean` | When using `omni --local` to prioritize local commands lookup over global ones, this variable will be set to `true`. It is not used by omni in any way, but can be used by scripts to determine if omni is running in local mode. |
