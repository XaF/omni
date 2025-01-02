---
description: Configuration of the `path_repo_updates/per_repo_config` parameter
---

# `per_repo_config`

## Parameters

Per-repository configuration that overrides the global configuration of how to update repositories in the path.
This is a map where the key is the repository identifier (e.g. `github.com:XaF/omni`) and the value is a map with the following parameters:

| Parameter  | Type           | Description                                       |
|------------|----------------|---------------------------------------------------|
| `enabled` | boolean | overrides whether the update is enabled for the repository |
| `ref_type` | enum: `branch` or `tag` | overrides the ref type for the repository |
| `ref_match` | regex | overrides the ref match for the repository |

## Example

```yaml
per_repo_config:
  github.com:omnicli/omni-example:
    enabled: true
    ref_type: "tag"
    ref_match: "^v[0-9]+\.[0-9]+\.[0-9]+"
```
