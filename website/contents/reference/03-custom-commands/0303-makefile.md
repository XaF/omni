---
description: Custom commands from Makefile
---

# Makefile commands

Omni supports parsing `Makefile` files in your current tree, while in a git repository, and exposing the `Makefile` targets as omni commands. This allows to make `omni` the go-to command, no matter if your project depended on a `Makefile` until now, as it will allow discovery of those commands as well.

:::info Current working directory
Makefile commands are run from the directory in which the `Makefile` is located to make sure that any relative path used in the `Makefile` will still be valid when running the command through omni.
:::

:::tip Scope
Makefile commands are scoped to the tree they are in. If you want to access a Makefile command from anywhere else, you can use [`omni scope`](/reference/builtin-commands/scope). Note that scoping to a repository will only load the `Makefile` available at the root of that repository.
:::

## From target to omni command

If omni scrapes the following `Makefile`:

```makefile
target1:
        @echo This is target1

target2:
        @echo This is target2
```

The following commands would be made available:
- `omni target1`
- `omni target2`

## `omni help`

By default, all those commands will appear in the `Uncategorized` section of the `omni help`, without any description. Running `omni help <command>` on any of those commands will, however, provide you with the exact `Makefile` location and the exact line of that `Makefile` where the target was scrapped from.

Omni supports special comments in the `Makefile` to categorize the commands and provide a short help to be shown in `omni help`.

### Setting categories for targets

Targets can be categorized by putting `##@ <category>` anywhere in the `Makefile`. All targets following that mark will be considered in the specified category.

#### Example

If omni scrapes the following `Makefile`, `omni help` will show:
- `target1` as `Uncategorized`
- `target2` and `target3` as `Category1`
- `target4` as `Category2`

```makefile
target1:
        @echo This is target1

##@ Category1

target2:
        @echo This is target2

target3: target2
        @echo This is target3

##@ Category2

target4:
        @echo This is target4
```

### Adding a help message for a target

It is possible to add help messages to be shown for a target by putting `## <help message>` on the same line as the target. The target in question will have that help message appear when calling `omni help` or `omni help <target>`.

#### Example

```makefile
target1: ## This is target 1
        @echo This is target1

##@ Category1

target2: ## This is target 2
        @echo This is target2

target3: target2 ## This is target 3
        @echo This is target3

##@ Category2

target4: ## This is target 4
        @echo This is target4
```

## Environment

The following environment variables are set by omni before the Makefile command is called:

| Environment variable | Type | Description |
|----------------------|------|-------------|
| `OMNI_SUBCOMMAND` | string... | The subcommand that was called leading to the execution of that command; e.g. `my command` for `omni my command` |
| `OMNI_CWD` | path | The current working directory where `omni` was called from |

The following environment variables are set by the shell integration and can be taken advantage of when writing commands:

| Environment variable | Type | Description |
|----------------------|------|-------------|
| `OMNI_SHELL` | string | The shell of the user for which the shell integration was loaded |
| `OMNI_CMD_FILE` | filepath | The file in which omni will read operations to apply to the shell; this needs to be compatible with the shell of the user as provided by `OMNI_SHELL` |
