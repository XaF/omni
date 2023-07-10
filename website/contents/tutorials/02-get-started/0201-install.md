---
description: How to install omni and get ready to use it
---

# Installation

Omni's installation requires to [get omni itself](#get-omni), and [setup its shell integration](#setting-up-the-shell-integration).

## Get omni

Omni can be installed in one of three ways:
- Using homebrew *(recommended, if available)*
- Downloading the pre-built binary
- Building from sources

### Using Homebrew

You can install omni using Homebrew or Linuxbrew if your architecture is `arm64` or `x86_64`. You simply need to run the following two commands:

```bash
brew tap xaf/omni
brew install omni
```

### Downloading the binary

Pre-built binaries are available for MacOS and Linux, for `arm64` and `x86_64` architectures. You can [download the last release binaries directly from the GitHub releases](https://github.com/XaF/omni/releases/).

### Using `cargo install`

Omni is [available as the `omnicli` cargo crate](https://crates.io/crates/omnicli).
You can thus install it by running the following command:

```bash
cargo install omnicli --root /path/to/bindir
```

:::caution
If installing omni through `cargo install`, make sure to install it in a path different from your `$CARGO_HOME`, as omni's dynamic environment might replace the `$CARGO_HOME/bin` directory in your `PATH` when loading a dynamic rust environment.
:::

### Building from sources

#### Clone the git repository

```bash
git clone https://github.com/XaF/omni
cd omni
```

#### Install Rust

```bash
curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh
```

See [the rust documentation](https://doc.rust-lang.org/book/ch01-01-installation.html) for more details.

#### Build and Install

```bash
cargo build --release
```

This will generate the `omni` binary in `target/release`. You can copy this binary anywhere in your `PATH`, e.g.:

```bash
cp target/release/omni /usr/local/bin/
```

## Setting up the shell integration

Omni depends on a shell integration to be fully functional. To enable it, you will have to add one of the following lines to your shell's configuration file:

```bash
eval "$(omni hook init bash)"    # for bash
eval "$(omni hook init zsh)"     # for zsh
omni hook init fish | source     # for fish
```

Don't forget to restart your shell or run `source <path_to_rc_file>` for the changes to take effect.

:::note
Support for other shells than the ones listed above can be added to omni in the future. Do not hesitate to submit a pull request with a template for supporting your shell if you use a different one.
:::
