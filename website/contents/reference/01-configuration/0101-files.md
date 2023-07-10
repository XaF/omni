---
description: Configuration files that omni is looking for
---

# Configuration files

The omni configuration files are expected to be in `YAML` format. They are searched for in the order they are listed below. Configuration options from later-applied files override configuration options from earlier-applied files.

Some configuration parameters will be ignored if present in the global configuration, others will be ignored if present in the per-repository configuration. The parameters for which it is the case are clearly identified in the documentation.

## Global configuration

- `~/.omni.yaml`
- `~/.config/omni.yaml` (will conform to the `XDG_CONFIG_HOME` environment variable, if set)
- `~/.config/omni/config.yaml`
- Any file which path is stored in the `OMNI_CONFIG` environment variable, if present and non-empty

:::note
If no configuration file is present when omni will need to edit one, the latest in the list will be used.
:::

## Per-repository configuration

From the root of the git repository:

- `.omni.yaml`
- `.omni/config.yaml`
