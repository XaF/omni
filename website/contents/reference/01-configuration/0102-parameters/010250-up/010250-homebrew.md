---
description: Configuration of the `homebrew` kind of `up` parameter
---

# `homebrew` operation

Taps Homebrew repositories and installs formulae and/or casks.

Omni will keep track of the repositories it taps (that weren't already available in the system)
and of the formulas and casks it installs. When running `omni down`, if some of those dependencies
were installed by omni and are no more used by any of the repositories `omni up`-ed, those will be
automatically uninstalled.

:::info
If `brew` is not available on the system, this step will be ignored.
:::

## Alternative names

- `brew`

## Parameters

| Parameter        | Type      | Description                                           |
|------------------|-----------|-------------------------------------------------------|
| `tap` | list of [tap](#tap) | List of repositories to tap |
| `install` | list of [install](#install) | List of formulae and casks to install |


### `tap`

| Parameter        | Type      | Description                                           |
|------------------|-----------|-------------------------------------------------------|
| `repo` | string | The name of the repository to tap, in the `<owner>/<repo>` format |
| `url` | string | The URL to tap the repository from (necessary if not following the `https://github.com/<owner>/homebrew-<repo>` format) |


### `install`

| Parameter        | Type      | Description                                           |
|------------------|-----------|-------------------------------------------------------|
| `formula` | string | The name of the formula to install (cannot be used along with `cask`) |
| `cask` | string | The name of the cask to install (cannot be used along with `formula`) |
| `version` | string | The version to install for the formula or cask |

## Examples

```yaml
up:
  # Will do nothing if no parameters are passed
  - homebrew

  # We can call it with the alternative name too
  - brew

  # We can decide to only install a number of formulas and casks
  - homebrew:
    # Regular formulas
    - bash
    - git
    # A formula from a tap, without tapping first
    - xaf/omni/omni
    # A formula with a version number; if the formula
    # with that name and version is not available directly
    # nor from a tap, omni will try and fetch that specific
    # version into a local tap to install it
    - formula: pnpm
      version: 8.6.3
    # And we can install a cask
    - cask: betterzip

  # We can also install formulas using the `install` key
  - homebrew:
      install:
        - bash
        - git

  # We can tap a repository before installing formulas
  - homebrew:
      tap:
        - xaf/omni
      install:
        - omni

  # We can also specify the url of the repository to tap if needed
  - homebrew:
      tap:
        - repo: xaf/omni
          url: https://github.com/XaF/omni
      install:
        - omni
```

## Dynamic environment

The following variables will be set as part of the [dynamic environment](/reference/dynamic-environment).

| Environment variable | Operation | Description |
|----------------------|-----------|-------------|
| `PATH` | prepend | For formulas, uses `$(brew --prefix --installed <formula>)/bin`; for casks, adds any `bin` directory containing at least one executable in the `$(brew --prefix)/Caskroom/<cask>` directory; in both cases, also injects the `$(brew --prefix)/bin` directory |
