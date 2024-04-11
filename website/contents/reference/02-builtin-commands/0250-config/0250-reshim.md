---
description: Builtin command `config reshim`
---

# `reshim`

Regenerate the shims for the environments managed by omni

This will get all the binaries that exist for at least one of the environments managed by omni and create a shim for them in the shim directory. This includes binaries imported by using any [`up` operation](/reference/configuration/parameters/up).

## Examples

```bash
# Regenerate the shims
omni config reshim
```
