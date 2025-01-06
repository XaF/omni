---
description: Builtin command `config check`
---

# `check`

The `check` command is used to check the configuration files and commands in the omnipath for errors. This command is useful for debugging and ensuring that the configuration is correct, or to lint your [omni commands metadatas](/reference/custom-commands/path/metadata).

## Parameters

| Parameter       | Required | Value type | Description                                         |
|-----------------|----------|------------|-----------------------------------------------------|
| `--search-path` | no | `string`   | Path to search for omni commands to validate the metadata of. Can be repeated. If neither this nor `--config-file` is specified, the current omnipath is checked. |
| `--config-file` | no | `string`   | Path to the configuration file to validate. Can be repeated. If neither this nor `--search-path` is specified, the current configuration is checked. |
| `--global` | no | `null` | Only validate the global configuration and global commands. |
| `--local` | no | `null` | Only validate the configuration and commands that are local to the current worktree. |
| `--include-packages` | no | `null` | Include the packages in the validation. |
| `--ignore` | no | `string` | Ignore the specified error codes. Can be repeated. Can be used to only specify a prefix of the error code, e.g. `--ignore=M` will ignore all metadata header errors. |
| `--select` | no | `string` | Only validate the specified error codes. Can be repeated. Can be used to only specify a prefix of the error code, e.g. `--select=M` will only validate metadata header errors. Some errors are ignored by default and need to be explicitly selected to appear in the reports. |
| `--pattern` | no | `string` | Only validate the files that match the specified pattern. Can be repeated. The pattern can start with `!` to exclude files. The patterns are processed in order and the first match is used. |
| `--output` | no | `plain` or `json` | Output format. Default is `plain`. |

## Examples

```bash
# Run a check of all the configuration files and commands, except for packages
omni config check

# Run a check of the global configuration files and commands, including packages
omni config check --include-packages

# Run a check of the worktree configuration files and commands
omni config check --local

# Run a check of the global configuration files and commands,
# ignoring all M-prefixed errors, except for the M0-prefixed errors
omni config check --ignore M --select M0
```

## Error codes

### Configuration errors

| Error code | Selected | Description |
|------------|--------|-------------|
| **`C0`** | | **Key-related errors** |
| `C001` | ✅ | Missing key in the configuration (e.g. is required but was not provided) |
| `C002` | ✅ | Empty key in the configuration (e.g. was provided but was empty) |
| `C003` | ✅ | Configuration option allows to specify an entry with a single-key-pair-table, but the table found does not have exactly one key |
| **`C1`** | | **Value-related errors** |
| `C101` | ✅ | Invalid value type in the configuration (e.g. expected a string but got a number) |
| `C102` | ✅ | Invalid value in the configuration (e.g. expected 'a' but got 'b') |
| `C103` | ✅ | Invalid range in the configuration (e.g. expected a value defining a range, but the range is invalid) |
| `C104` | ✅ | Invalid package in the configuration (e.g. expected a package name but got a value that can't resolve to a package) |
| `C110` | ✅ | Unsupported value in the configuration (e.g. a value is not supported in the current context) |
| `C120` | ✅ | Parsing error in the configuration (e.g. failed to parse a value) |

### Metadata errors

| Error code | Selected | Description |
|------------|--------|-------------|
| **`M0`** | | **File-scoped metadata errors** |
| `M001` | ✅ | Metadata header is missing the `help` key |
| `M002` | ❌ | Metadata header is missing the `syntax` key *(ignored by default, needs to be explicitly selected)* |
| **`M1`** | | **Key-related errors for the metadata** |
| `M101` | ✅ | Metadata header has an unknown key |
| `M102` | ✅ | Metadata header is missing a subkey |
| `M103` | ✅ | Metadata header has a continuation without a key |
| `M104` | ✅ | Metadata header has a duplicate key |
| **`M2`** | | **Value-related errors for the metadata** |
| `M201` | ✅ |  Metadata header has an invalid value type |
| **`M3`** | | **Definition of `arggroup`** |
| `M301` | ✅ | Metadata header group is missing parameters |
| `M308` | ✅ | Metadata header group has an empty part |
| `M309` | ✅ | Metadata header group has an unknown config key |
| **`M4`** | | **Definition of `arg` or `opt`** |
| `M401` | ✅ | Metadata header parameter has an invalid key-value pair |
| `M402` | ✅ | Metadata header parameter is missing a description |
| `M408` | ✅ | Metadata header parameter has an empty part |
| `M409` | ✅ | Metadata header parameter has an unknown config key |

### Path errors

| Error code | Selected | Description |
|------------|--------|-------------|
| `P001` | ✅ | Path does not exist |
| `P002` | ✅ | A file present in the path is not executable |
| `P003` | ✅ | A file present in the path does not have metadata, or they couldn't be loaded |
