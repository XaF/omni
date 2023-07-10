---
description: Import your repositories into omni
---

# Import repositories

Once omni is installed and configured, you might have decided to use a different worktree than you were using before, or just want your current worktree tidied up using the [`repo_path_format`](/reference/configuration/parameters/repo_path_format) that you just configured.

Omni provides a simple helper command for this:

```bash
omni tidy
```

When running that command, all the repositories in your configured worktrees will be identified, and omni will offer you to reorganize them into the directories they should be located at. If two repositories would end up in the same location, omni will skip one of the two and let you know.

If some of your repositories are at a different path than any of your configured worktrees, you can use the following command to specify where to look for repositories:

```bash
omni tidy --search-path <path>
```

The `--search-path` parameter can be repeated multiple times.

:::tip
Even if all your repositories are identifiable by omni, importing them into the structure you defined with the [`repo_path_format`](/reference/configuration/parameters/repo_path_format) parameter will make commands such as `omni cd` faster, as repositories are first searched through known expected paths before doing a file system search.
:::
