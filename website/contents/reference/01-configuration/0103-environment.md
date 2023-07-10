---
description: Environment variables that can be used with omni
---

# Environment variables

Omni supports a number of environment variables for its configuration. Setting those will generally override equivalent configuration options or precede them.

| Variable                | Type | Description                                                            |
|-------------------------|------|------------------------------------------------------------------------|
| `OMNI_GIT` | `path` | The worktree where omni will clone and look for repositories. Overrides the configuration. See [parameters/worktree](parameters/worktree#environment) |
| `OMNI_ORG` | comma-delimited list of strings | Prepend organizations to be considered by omni. e.g.: `OMNI_ORG="git@github.com:XaF,github.com/XaF"`. See [parameters/org](parameters/org#environment) for more details. |
| `OMNI_CONFIG` | `filepath` | The path to an omni global configuration file. See [files](files#global-configuration). |
| `OMNIPATH` | colon-delimited list of paths | Provides the paths to different omni commands. See [parameters/path](parameters/path#environment) for more details. |
