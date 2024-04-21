---
description: Configuration of the `asdf` parameter
---

# `asdf`

## Parameters

Configuration of the cache for `asdf` operations.

| Operation | Type | Description                                                    |
|-----------|------|---------------------------------------------------------|
| `versions_expire` | duration | How long to cache a given GitHub repository versions for. This allows to avoid listing available versions on each `omni up` call. The versions are automatically re-listed if the cache does not contain any matching version. |
| `cleanup_after` | duration | The grace period before cleaning up the resources that are no longer needed. |

## Example

```yaml
asdf:
  versions_expire: 1d
  cleanup_after: 1w
```
