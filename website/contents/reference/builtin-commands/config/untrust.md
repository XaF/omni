---
description: Builtin command `config untrust`
---

# `untrust`

Untrust a work directory.

When a work directory is not trusted, `omni up` and any work directory-provided commands will require confirmation before each run.

## Parameters

| Parameter       | Required | Value type | Description                                         |
|-----------------|----------|------------|-----------------------------------------------------|
| `--check` | no | `null` | If provided, will only check the current status of trust for the repository; if the repository is trusted, exit code will be `0`, if the repository is not trusted, it will be `2` and in case of error it will be `1` |
| `repo` | no | string | The name of the repo to change directory to; this can be in the format of a full git URL, or `<org>/<repo>`, or just `<repo>`, in which case the repo will be searched for in all the organizations in the order in which they are defined, and then trying all the other repositories in the configured worktrees. |

## Examples

```bash
# Trust the current repository
omni config untrust

# Trust the xaf/omni repository
omni config untrust xaf/omni

# Check the trust status of the current repository
omni config untrust --check

# Check the trust status of the xaf/omni repository
omni config untrust --check xaf/omni
```
