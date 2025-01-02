---
description: Configuration of the `and` kind of `up` parameter
---

# `and` operation

Composite operation that takes a list of operations as parameter.

The `and` operation will execute all available operations in the list. If none of the operations are available, the `and` operation will be considered unavailable. If any of the operations fail, the `and` operation will fail.

By default the `up` configuration is considered an `and` operation, it is thus not necessary to specify it explicitly at the root level. However, it becomes useful when combined with the [`or`](or) operation, as it allows to specify grouped operations to be conditioned by `or`.

## Examples

```yaml
up:
  # Installs gawk using either homebrew or nix, whichever is available,
  # and if both are available will use homebrew in priority, and will
  # run a different custom operation depending on which one was used;
  #
  # NOTE: if the custom operation fails, the whole `and` operation will
  #       be considered as failed, and the other branching of `or` will
  #       be executed.
  - or:
    - and:
      - homebrew:
          install:
          - gawk
      - custom:
          run: echo "gawk is installed using homebrew"
    - and:
      - nix:
        - gawk
      - custom:
          run: echo "gawk is installed using nix"
```
