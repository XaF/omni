---
description: Omni's dynamic environment for git repositories
---

# Dynamic environment

Omni provides a dynamic environment for repositories. During `omni up`, omni will store the environment that needs loading when you navigate to the current repository.

## Configuration

The following configuration parameters lead to dynamic environment.

| Configuration | Dynamic environment |
|---------------|---------------------|
| `env` | Each entry in the map leads to setting an environment variable to the defined value |
| [`bash` operation](/reference/configuration/parameters/up/bash) | [See details](/reference/configuration/parameters/up/bash#dynamic-environment) |
| [`bundler` operation](/reference/configuration/parameters/up/bundler) | [See details](/reference/configuration/parameters/up/bundler#dynamic-environment) |
| [`go` operation](/reference/configuration/parameters/up/go) | [See details](/reference/configuration/parameters/up/go#dynamic-environment) |
| [`node` operation](/reference/configuration/parameters/up/node) | [See details](/reference/configuration/parameters/up/node#dynamic-environment) |
| [`python` operation](/reference/configuration/parameters/up/python) | [See details](/reference/configuration/parameters/up/python#dynamic-environment) |
| [`ruby` operation](/reference/configuration/parameters/up/ruby) | [See details](/reference/configuration/parameters/up/ruby#dynamic-environment) |
| [`rust` operation](/reference/configuration/parameters/up/rust) | [See details](/reference/configuration/parameters/up/rust#dynamic-environment) |
| [`terraform` operation](/reference/configuration/parameters/up/terraform) | [See details](/reference/configuration/parameters/up/terraform#dynamic-environment) |

## Behind the scene

### The `__omni_dynenv` environment variable

Omni's dynamic environment is set when entering an `omni up`-ed repository directory, and updated or unset when leaving this directory. Omni uses the `__omni_dynenv` environment variable to keep track of this.

This variable is structured with a [blake3](https://github.com/BLAKE3-team/BLAKE3) hash, semicolon-separated from a JSON object. The hash allows to easily identify if the current dynamic environment corresponds to the expected one. The JSON object indicates all the changes that have been operated on the environment.

This is inspired from [the way `shadowenv` keeps track of environment changes](https://shopify.github.io/shadowenv/integration/) so it can restore the environment when leaving a directory.
