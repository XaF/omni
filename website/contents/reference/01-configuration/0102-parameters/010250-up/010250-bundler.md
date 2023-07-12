---
description: Configuration of the `bundler` kind of `up` parameter
---

# `bundler` operation

Install bundler dependencies.

## Alternative names

- `bundle`

## Parameters

The following parameters can be used:

| Parameter        | Type      | Description                                           |
|------------------|-----------|-------------------------------------------------------|
| `gemfile` | string | Path to the `Gemfile` file; if not provided, defaults to `Gemfile` at the root of the git repository |
| `path` | string | Path to the vendor directory where the dependencies will be installed; if not provided, defaults to `vendor/bundle` |

## Examples

```yaml
up:
  # Defaults to use the Gemfile in the repository root, and
  # to vendor in `vendor/bundle`
  - bundler

  # We can call it with the alternative name too
  - bundle

  # Or we can specify a different location for the Gemfile
  - bundler: alt/Gemfile

  # Or specify that location with the direct parameter
  - bundler:
      gemfile: alt/Gemfile

  # And we can specify the path to put the vendored dependencies
  - bundler:
      path: vendor/bundle
```

## Dynamic environment

The following variables will be set as part of the [dynamic environment](/reference/dynamic-environment).

| Environment variable | Operation | Description |
|----------------------|-----------|-------------|
| `BUNDLE_GEMFILE` | set | The location of the Gemfile |
