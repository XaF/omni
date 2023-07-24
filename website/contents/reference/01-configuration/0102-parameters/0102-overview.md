---
description: Parameters that can be used in the configuration files
slug: /reference/configuration/parameters
---

# Parameters

## List of configuration parameters

Omni configuration files accept the following parameters:

| Parameter               | Type | Description                                                            |
|-------------------------|------|------------------------------------------------------------------------|
| `cache` | string | Location of the cache file used by omni |
| `cd` | [cd](parameters/cd) | Configuration related to the `omni cd` command |
| `clone` | [clone](parameters/clone) | Configuration related to the `omni clone` command |
| `command_match_min_score` | float | the minimum score to be considered when fuzzy matching a command |
| `command_match_skip_prompt_if` | [*_skip_prompt_if](parameters/skip-prompt-if) | Configuration of prompt skipping when fuzzy matching a command |
| `commands` | [commands](parameters/commands) (map) | Commands made available through omni |
| `config_commands` | [config_commands](parameters/config_commands) | Configuration related to the commands defined in the config file |
| `env` | map | A key-value map of environment variables to be set when running omni commands |
| `makefile_commands` | [makefile_commands](parameters/makefile_commands) | Configuration related to the commands generated from Makefile targets |
| `org` | [org](parameters/org) (list) | Configuration for the default organizations |
| `path_repo_updates` | [path_repo_updates](parameters/path_repo_updates) | Configuration for the automated updates of the repositories in omni path |
| `path` | [path](parameters/path) | Configuration of the omni path |
| `repo_path_format` | [repo_path_format](parameters/repo_path_format) (string) | How to format repositories when cloning them with `omni clone` or searching them with `omni cd` *(default: `%{host}/%{org}/%{repo}`)* |
| `suggest_config` | [suggest_config](parameters/suggest_config) | Configuration that a git repository suggests should be added to the user configuration. *Should only be used in git repositories configuration.* |
| `suggest_clone` | [suggest_clone](parameters/suggest_clone) | Repositories that a git repository suggests should be clone. *Should only be used in git repositories configuration.* |
| `up` | [up](parameters/up) (list) | List of operations needed to set up or tear down a repository |
| `worktree` | [worktree](parameters/worktree) (string) | Default location of the worktree, where the git repositories are expected to be located |

## Examples

### Simple user configuration

```yaml
org:
  - handle: "git@github.com:XaF"
    trusted: true
  - handle: "git@github.com:omnicli"
    trusted: true
path:
  append:
    - /Users/xaf/git/omnicli/omni-example
repo_path_format: "%{org}/%{repo}"
```

### All values set by the default configuration

```yaml
commands: {}
command_match_min_score: 0.12
command_match_skip_prompt_if:
  enabled: true
  first_min: 0.80
  second_max: 0.60
cd:
  path_match_min_score: 0.12
  path_match_skip_prompt_if:
    enabled: true
    first_min: 0.80
    second_max: 0.60
clone:
  ls_remote_timeout_seconds: 5
config_commands:
  split_on_dash: true
  split_on_slash: true
env: {}
makefile_commands:
  enabled: true
  split_on_dash: true
  split_on_slash: true
org: []
path:
  append: []
  prepend: []
path_repo_updates:
  enabled: true
  self_update: ask # true, false or ask
  interval: 43200 # 12 hours
  ref_type: "branch" # branch or tag
  ref_match: null # regex or null
  per_repo_config: {}
repo_path_format: "%{host}/%{org}/%{repo}"
```
