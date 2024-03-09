---
description: Configuration of the `prompts` parameter
---

# `prompts`

## Parameters

List of prompts to be asked before running the operations and suggesting anything during a call to `omni up`.

Each prompt configuration can use [templates](/reference/configuration/templates), including the already-asked prompts. This can be used to ask a prompt only if another prompt has been answered in a certain way for example.

| Parameter | Type | Description                                                    |
|-----------|------|----------------------------------------------------------------|
| `id` | string | *Required.* The identifier of the prompt, must be unique as will be used to identify the answer to this prompt, or if that prompt has already been answered before. |
| `prompt` | string | *Required.* The prompt to be asked. |
| `scope` | string | The scope of the prompt, can be `repo` or `org` (*default: `repo`*). If the scope is `repo`, it means that the answer to this prompt will only be considered for this repository. If the scope is `org`, the prompts from other repositories of the same `org` will be considered. This allows to load already-answered prompts instead of asking again. |
| `type` | [prompt type](#prompt-types) | The type of the prompt, can be `boolean`, `string`, `number`, `password` or `list` (*default: `boolean`*).
| `if` | string | The condition to ask the prompt. The condition is a template that must resolve to `true`, `yes`, `on` or `1`. If it resolves to anything else, the prompt will not be asked. |
| `default` | string | The default value of the prompt. |
| `choices` | list([choice](#choice)) or string | *Required for `choice` and `multichoice` prompts.* The list of choices for the prompt. Can be provided as a string template of a list to dynamically add options. |

### Prompt types

| Prompt type | Template type | Description |
|-------------|---------------|-------------|
| `text` | string | A string prompt, the answer can be any string. |
| `password` | string | A password prompt, the answer can be any string. The typed characters will be hidden. *Note that the prompt result will be stored in clear in the cache file.* |
| `confirm` | boolean | A confirmation prompt, the answer can be `yes` or `no`. |
| `choice` | string | A list prompt, the answer can be any of the items in the list. |
| `multichoice` | list | A multiple choice prompt, the answer can be any of the items in the list. |
| `int` | int | An integer prompt, the answer can be any integer. |
| `float` | float | A float prompt, the answer can be any float. |

### Choice

Choice for a `choice` or `multichoice` prompt.

Can be provided as a string or an object with the following parameters:

| Parameter | Type | Description |
|-----------|------|-------------|
| `id` | string | The identifier of the choice. The identifier is used in templates when reading the prompt value. If not provided, the value of the choice will be used as the identifier. |
| `choice` | string | The choice to be displayed to the user. If not provided, the identifier will be displayed. |

## Example

```yaml
prompts:
- id: team
  prompt: What is your team?
  scope: org
  type: choice
  choices:
  - id: team1
    choice: team 1
  - team 2
  - team 3
- id: subteam
  prompt: What are your subteams in {{ prompts.team }}?
  scope: org
  type: multichoice
  choices: |
    - subteam 1
    - subteam 2
    {{% if prompts.team == "team1" %}}- subteam 3{{% endif %}}
  if: '{{ prompts.team == "team1" or prompts.team == "team 2" }}'
```
