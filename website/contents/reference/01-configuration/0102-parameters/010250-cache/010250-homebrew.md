---
description: Configuration of the `homebrew` parameter
---

# `homebrew`

## Parameters

Configuration of the cache for `homebrew` operations.

| Operation | Type | Description                                                    |
|-----------|------|---------------------------------------------------------|
| `update_expire` | duration | How long to cache the fact that `brew update` has been run. This allows to avoid running it on each `omni up` call. |
| `install_update_expire` | duration | How long to cache the fact that `brew upgrade` has been run for a given formulae or cask. This allows to avoid running it on each `omni up` call. |
| `install_check_expire` | duration | How long to cache that we have seen a given formulae or cask as installed. This allows to avoid checking it on each `omni up` call. |

## Example

```yaml
homebrew:
  update_expire: 1d
  install_update_expire: 1d
  install_check_expire: 12h
```
