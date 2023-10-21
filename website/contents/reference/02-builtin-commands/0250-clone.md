---
description: Builtin command `clone`
---

# `clone`

Clone the specified repository

The clone operation will be handled using the first organization that matches the argument and for which the
repository exists. The repository will be cloned in a path that matches omni's expectations, depending on your
configuration.

Upon successful cloning, `omni clone` will run `omni up --update-user-config` in the just-cloned repository ([unless disabled in the configuration](/reference/configuration/parameters/clone)) and will change directory to the newly cloned repository.

## Parameters

### Arguments

| Argument        | Value type | Description                                         |
|-----------------|------------|-----------------------------------------------------|
| `repo` | string | The repository to clone; this can be in format `<org>/<repo>`, just `<repo>`, or the full URL. If the case where a full URL is not specified, the configured organizations will be used to search for the repository to clone. |

### Options

| Option          | Value type | Description                                         |
|-----------------|------------|-----------------------------------------------------|
| `--package`  | bool | Clone the repository as a package, instead of the worktree |
| `options...` | any | Any additional options to pass to git clone. |

## Examples

```bash
# Very basic cloning, except that the repository will be placed in the correct worktree,
# with the expected repo_path_format
omni clone https://github.com/XaF/omni

# If we have github.com somewhere in our organizations, we can also simply run
omni clone XaF/omni

# And if we have defined https://github.com/XaF as an organization, this will work
omni clone omni

# We can also specify to use a different branch than the default one
omni clone omni --branch some_other_branch

# Or really, any other parameter that `git clone` supports
omni clone omni --depth 1

# Clone the repository as a package
omni clone --package https://github.com/XaF/omni
```
