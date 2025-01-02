---
description: Configuration of the `cd` parameter
---

# `cd`

## Parameters

Configuration related to the `omni cd` command.

| Parameter       | Type      | Description                                         |
|-----------------|-----------|-----------------------------------------------------|
| `fast_search` | bool | Whether or not to enable fast search for repositories lookup *(default: true)* |
| `path_match_min_score` | float | the minimum score to be considered when fuzzy matching a repository path |
| `path_match_skip_prompt_if` | [*_skip_prompt_if](skip-prompt-if) | Configuration of prompt skipping when fuzzy matching a repository path |

## Example

```yaml
cd:
  fast_search: true
  path_match_min_score: 0.12
  path_match_skip_prompt_if:
    enabled: true
    first_min: 0.80
    second_max: 0.60
```
