---
description: IDE integration
---

import Tabs from '@theme/Tabs';
import TabItem from '@theme/TabItem';

# IDE integration

Integrated Development Environments (IDEs) are powerful tools that can help you write code faster and more efficiently. However, they are not always compatible with omni's dynamic environment. This document will try to guide you through the process of integrating omni with your IDE, but there are no guarantees that omni will always work out of the box with your IDE.

Omni relies heavily on its dynamic environment to manage the tools and dependencies of your projects. This dynamic environment is set up by omni's shell integration, which is not always compatible with IDEs that won't load your shell environment _before_ running its tools. Most of the time, IDEs will use the environment they have when launched.

To work around this limitation, omni provides shims, which are fake binaries of the tools managed by omni, that omni controls, allowing for calls to those to resolve to the expected version depending on your current working directory. For instance, if you have a project that uses a specific version of Rust, and you have that version installed with omni, the `cargo` shim will resolve to that version when you run `cargo` in your project's directory, or fallback to any system version (or `command not found` if none) if you're not in a project with rust managed by omni.

:::info
This is a moving documentation, so please don't hesitate to open pull requests to add your IDE or improve the suggestions for existing ones.
:::

## Using shims

Omni defaults to load shims, and they get removed from the path automatically by omni's dynamic environment. This allows for non-interactive shells to default to use shims, while interactive shells will use direct path entries.

### Keep shims in the path

In case you launch your IDE from your shell, if you don't have any other way to add an extra path to its environment, you can make sure that omni keeps its shims in its path by using the `--keep-shims` option when running `omni hook init`:

<Tabs groupId="shells">
  <TabItem value="bash" label="bash" default>
    ```bash
    eval "$(omni hook init --keep-shims bash)"
    ```
  </TabItem>
  <TabItem value="zsh" label="zsh">
    ```bash
    eval "$(omni hook init --keep-shims zsh)"
    ```
  </TabItem>
  <TabItem value="fish" label="fish">
    ```bash
    omni hook init --keep-shims fish | source
    ```
  </TabItem>
</Tabs>

### Use shims only

Another possibility is to default to use shims only instead of the dynamic environment. This is not recommended as shims are not as flexible as the dynamic environment. However, if you're in a situation where you can't use the dynamic environment, you can use the `--shims` option when running `omni hook init`:

<Tabs groupId="shells">
  <TabItem value="bash" label="bash" default>
    ```bash
    eval "$(omni hook init --shims bash)"
    ```
  </TabItem>
  <TabItem value="zsh" label="zsh">
    ```bash
    eval "$(omni hook init --shims zsh)"
    ```
  </TabItem>
  <TabItem value="fish" label="fish">
    ```bash
    omni hook init --shims fish | source
    ```
  </TabItem>
</Tabs>


## Specific IDEs

Each of those methods rely on adding manually the shims path to the IDE's environment.

Omni defaults to put the shims into its data path, which is usually `~/.local/share/omni/shims`, but follows the `OMNI_DATA_HOME` or `XDG_DATA_HOME` environment variables if set. If you can set the environment variable from a command execution, you can use the following command to get the shims path:

```bash
omni hook init --print-shims-path
```


### [Vim](https://www.vim.org/)

```vim
" Add omni shims to the path
let $PATH = system('omni hook init --print-shims-path') . ':' . $PATH
```


### [Neovim](https://neovim.io/)

```lua
-- Add omni shims to the path
vim.env.PATH = vim.fn.system('omni hook init --print-shims-path') .. ':' .. vim.env.PATH
```


### [emacs](https://www.gnu.org/software/emacs/)

```lisp
;; Add omni shims to the path
(setenv "PATH" (concat (shell-command-to-string "omni hook init --print-shims-path") ":" (getenv "PATH")))
(setq exec-path (append (split-string (shell-command-to-string "omni hook init --print-shims-path") ":") exec-path))
```


### [Xcode](https://developer.apple.com/xcode/)

Xcode allows projects to run system commands as part of the build process. Commands are then run in a sandboxed environment using `/usr/bin/sandbox-exec`.

For your build steps, you can add a `Run Script` phase to your target, and use the following script to add the shims path to the environment:

```bash
eval "$(omni hook init --shims)"

# Your build steps here, that will now have access to the shims
```


### [JetBrains](https://www.jetbrains.com/)

:::info
This includes all JetBrains IDEs, such as [IntelliJ IDEA](https://www.jetbrains.com/idea), [PyCharm](https://www.jetbrains.com/pycharm), [WebStorm](https://www.jetbrains.com/webstorm), [GoLand](https://www.jetbrains.com/goland), [RubyMine](https://www.jetbrains.com/rubymine), [RustRover](https://www.jetbrains.com/rustrover), etc. as they share the same interface and behavior.
:::

JetBrains IDEs require you to select the SDK you want to use for your project. They support an `asdf` integration, which is the backend omni uses for a number of tools. If you are not otherwise using `asdf` yourself, you can create a symlink from omni's `asdf` directory to the `asdf` directory in your home directory:

```bash
ln -s ~/.local/share/omni/asdf ~/.asdf
```

Then, you can select the SDK you want to use for your project by selecting the `asdf` SDK of the expected version in `Project Settings` > `Project` > `SDK`.

For some tools, like `node`, this might show under `Languages & Frameworks` instead (for instance, under `Node.js` > `Node interpreter` for `node`).


### [Visual Studio Code](https://code.visualstudio.com/)

Visual Studio Code should work out of the box with omni shims when launched via a UI gesture, since it will start a small process to resolve the shell environment defined in your `.bashrc` or `.zshrc` files.

However, if you launch Visual Studio Code from the command line, it will use your current shell environment, that won't have the shims if you don't use the `--keep-shims` option when running `omni hook init`. To work around that, you can however create shell aliases for the `code` command that will load the shims:

<Tabs groupId="shells">
  <TabItem value="bash" label="bash" default>
    ```bash
    alias code='PATH=$(omni hook init --print-shims-path):$PATH code'
    ```
  </TabItem>
  <TabItem value="zsh" label="zsh">
    ```bash
    alias code='PATH=$(omni hook init --print-shims-path):$PATH code'
    ```
  </TabItem>
  <TabItem value="fish" label="fish">
    ```bash
    function code
        set -x PATH (omni hook init --print-shims-path) $PATH
        command code $argv
        set -e PATH
    end
    ```
  </TabItem>
</Tabs>

