---
description: Builtin commands
slug: /reference/builtin-commands
---

# Builtin commands

Omni provides a number of built-in commands. Those are available from anywhere omni can be called, and will be called from the current working directory.

Those commands take precedence over any custom commands, makefile commands or configuration commands, it is thus not possible to override them. However, you can still write "subcommands" for those, that would be called when the specific subcommand is being called.

## List of builtin commands

### General

| Builtin command         | Description                                               |
|-------------------------|-----------------------------------------------------------|
| [`config bootstrap`](builtin-commands/config/bootstrap) | Bootstraps the configuration of omni |
| [`config path switch`](builtin-commands/config/path/switch) | Switch the source of a repository in the omnipath |
| [`config reshim`](builtin-commands/config/reshim) | Regenerate the shims for the environments managed by omni |
| [`config trust`](builtin-commands/config/trust) | Trust a work directory |
| [`config untrust`](builtin-commands/config/untrust) | Untrust a work directory |
| [`help`](builtin-commands/help) | Show help for omni commands |
| [`hook`](builtin-commands/hook) | Call one of omni's hooks for the shell |
| [`status`](builtin-commands/status) | Show the status of omni |

### Git commands

| Builtin command         | Description                                               |
|-------------------------|-----------------------------------------------------------|
| [`cd`](builtin-commands/cd) | Change directory to the git directory of the specified repository |
| [`clone`](builtin-commands/clone) | Clone the specified repository |
| [`down`](builtin-commands/down) | Tear down a repository depending on its up configuration |
| [`scope`](builtin-commands/scope) | Runs an omni command in the context of the specified repository |
| [`tidy`](builtin-commands/tidy) | Organize your git repositories using the configured format |
| [`up`](builtin-commands/up) | Sets up a repository depending on its up configuration |

