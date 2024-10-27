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
|-------------------------|------|------------------------------------------------------------------------|
| `OMNI_LOCAL_LOOKUP` | `boolean` | When using `omni --local` to prioritize local commands lookup over global ones, this variable will be set to `true`. It is not used by omni in any way, but can be used by scripts to determine if omni is running in local mode. |
| `OMNI_ARG_LIST` | `string` | The list of arguments parsed by the argument parser for the command. |
| `OMNI_ARG_<argname>_TYPE` | `string` | The type of the argument `<argname>` parsed by the argument parser for the command. Can be one of `str`, `int`, `float`, `bool` for single-value arguments, or any `<type>/<size>` where `<type>` is one of the previous types and `<size>` is the number of values for multi-value arguments. |
| `OMNI_ARG_<argname>_VALUE` | `string` | The value of the argument `<argname>` parsed by the argument parser for the command, if the type is a single-value type. |
| `OMNI_ARG_<argname>_VALUE_<index>` | `string` | The value at index `<index>` of the argument `<argname>` parsed by the argument parser for the command, if the type is a multi-value type. The index is 0-based. |
