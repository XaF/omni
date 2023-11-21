---
description: Configuration of the `shell_aliases` parameter
---

# `shell_aliases`

Configuration of the shell aliases to be injected by the init hook.

## Parameters

This is expected to be a list of objects containing the following parameters:

| Parameter       | Type      | Description                                         |
|-----------------|-----------|-----------------------------------------------------|
| `alias` | string | The alias to be injected (e.g. `o`, `ocd`) |
| `target` | string | The command to alias to; if not specified, the alias will simply target omni |

## Example

```yaml
shell_aliases:
  # Create a shell alias `o` which targets `omni`
  - o

  # Create a shell alias `o2` which targets `omni`
  - alias: o2

  # Create a shell alias `ocd` which targets `omni cd`
  - alias: ocd
    target: cd
```
