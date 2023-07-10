---
description: Configuration of the `repo_path_format` parameter
---

# `repo_path_format`

## Parameters

How to format repositories when cloning them with `omni clone` or searching them with `omni cd`.
The value is a string and will be appended to the worktree in which the repository is to be cloned (or expected to be found). This is thus a **relative path**. You can use the following placeholder in the string template:

| Parameter | Description                                |
|-----------|--------------------------------------------|
| `%{host}` | Registry hostname (e.g. `github.com`) |
| `%{org}` | Repository owner (e.g. `XaF`) |
| `%{repo}` | Repository name (e.g. `omni`) |

If left unset, the default value for `repo_path_format` is `%{host}/%{org}/%{repo}` (e.g. `github.com/XaF/omni`).

## Examples

```yaml
# Most specific path
repo_path_format: "%{host}/%{org}/%{repo}"

# If we don't want to see the host
repo_path_format: "%{org}/%{repo}"

# If we want to put the repositories in a special 'repos' directory
repo_path_format: "%{org}/repos/%{repo}"
```
