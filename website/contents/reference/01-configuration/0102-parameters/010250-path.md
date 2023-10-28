---
description: Configuration of the `path` parameter
---

# `path`

Configuration to build the omni path, where omni will look for commands to make available to the user.

Once built, the omni path works like the `PATH` environment variable but for omni commands: only the first command in the path for a given name will be considered.

## Parameters

| Parameter  | Type           | Description                                       |
|------------|----------------|---------------------------------------------------|
| `append` | list of path entries | List of the paths to append to the omni path |
| `prepend` | list of path entries | List of the paths to prepend to the omni path |

If you want to be able to stack paths in different configuration files, you can take advantage of [the configuration merging strategies](suggest_config#configuration-merging-strategies).

### Path entries

Each path entry can either be a string or a map with the following keys:

| Parameter  | Type           | Description                                       |
|------------|----------------|---------------------------------------------------|
| `package` | string | Handle of the repository to be used as a package; if provided and `path` is relative, the package path will be prepended to the value of `path` to compute the absolute path to be considered. |
| `path` | string | The absolute or relative path to the directory to be added to the omni path. If relative and not provided alongside a `package` value, will be considered as a relative path from the directory of the configuration file containing the path entry. |

## Example

```yaml
path:
  append:
    - /absolute/path
    - relative/path
    - path: /other/absolute/path
    - package: git@github.com:XaF/omni
      path: path/in/the/package
  prepend:
    - /absolute/path
    - relative/path
    - path: other/relative/path
```

## Environment

The environment variable `OMNIPATH` can be used to add paths as a colon-separated list. Any path added through the `OMNIPATH` environment variable will be considered after `path/prepend` and before `path/append`.

```bash
export OMNIPATH=/absolute/path1:/absolute/path2
```
