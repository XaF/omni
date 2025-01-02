---
description: Builtin command `down`
---

# `down`

Tears down a repository depending on its `up` configuration.

The steps to tear down the work directory are defined in the [`up` configuration parameter](/reference/configuration/parameters/up) of the [work directory configuration file](/reference/configuration/files#per-work-directory-configuration). Those steps are followed in the **reverse** order from which they are defined when running `omni down`.

Running this command will also clear the [dynamic environment](/reference/dynamic-environment) of the repository in which it is being run, and cleanup unused dependencies that omni installed during previous `omni up` calls.


:::info
**This needs to be run from a git repository.**
:::

:::note
`omni down` will run steps configured in the `up` configuration **in reverse**, tearing down the last step before the previous one. This allows some of your later steps to depend on dependencies installed in earlier steps, while still being torn down properly.
:::

## Examples

```bash
# Simply run the up steps for that repository
omni down
```
