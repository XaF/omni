---
description: Configuration of the `*_skip_prompt_if` parameters
---

# `*_skip_prompt_if`

## Parameters

Configuration to skip prompting the user when fuzzy matching in case the first match seems close enough, and the second match is far enough of the first one that we can consider the first one is what was wanted.

| Parameter       | Type      | Description                                         |
|-----------------|-----------|-----------------------------------------------------|
| `enabled` | boolean | whether or not to enable skipping the prompt if the conditions are met |
| `first_min` | float | the minimum matching rate for the closest match, between 0 and 1 *(default: 0.80)* |
| `second_max` | float | the maximum matching rate for the second closest match, between 0 and 1 *(default: 0.60)* |

## Example

```yaml
path_match_skip_prompt_if:
  enabled: true
  first_min: 0.80
  second_max: 0.60
```
