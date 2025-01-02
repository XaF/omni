---
description: Configuration of the `or` kind of `up` parameter
---

# `or` operation

Composite operation that takes a list of operations as parameter.

When called during `omni up`, it will execute operations in the list until one of them is available and succeeds. If none of the operations are available, the `or` operation will be considered unavailable. If none of the available operations succeed, the `or` operation will fail.

When called during `omni down`, it will execute all operations in the list.

## Examples

```yaml
up:
  # Installs gawk using either homebrew or nix, whichever is available,
  # and if both are available will use homebrew in priority
  - or:
    - homebrew:
        install:
        - gawk
    - nix:
      - gawk
```
