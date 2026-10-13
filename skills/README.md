# Agent skills

Skills for AI agents that model in SolidCraft through its [MCP server](../docs/mcp.md). Each folder is
one skill in the [Agent Skills](https://agentskills.io) format: a `SKILL.md` with a name and a
description that tell the agent when to load it, and an optional supporting reference file.

| Skill | Loads when the agent |
|---|---|
| [`solidcraft-modeling`](solidcraft-modeling/SKILL.md) | builds a new part (enclosure, bracket, plate, flange, …) |
| [`solidcraft-editing`](solidcraft-editing/SKILL.md) | changes, fixes or extends an existing design |

They teach the agent to:
- plan the part as numbers
- build it in a few `batch` calls with parametric positions
- verify every stage with `measure` against values it computed itself
- skip the large replies it doesn't need, such as the unfiltered command catalog and full topology dumps

They also list the traps that otherwise cost failed calls. Examples are the sketch axes of the XZ plane, edge picks that snap to the nearest edge, and cuts that miss but still report success.

They are written for SolidCraft 0.8 and later.

## Install

Claude Code, for all projects:

```sh
git clone --depth 1 https://github.com/bherbruck/solidcraft
cp -r solidcraft/skills/solidcraft-* ~/.claude/skills/
```

Windows (PowerShell):

```powershell
git clone --depth 1 https://github.com/bherbruck/solidcraft
Copy-Item -Recurse solidcraft\skills\solidcraft-* $HOME\.claude\skills\
```

- **One project only:** copy the folders into that project's `.claude/skills/` instead.
- **Other agents:** for agents that read the Agent Skills format, copy the folders into the skills directory their documentation names.
- **Updating:** copy the folders again after you update SolidCraft.

## Maintaining

- The worked example in `solidcraft-modeling/reference.md` runs in CI. `skill_reference_example_builds_as_documented` in `crates/mcp/src/tests.rs` builds it, checks the documented volume, changes a parameter and checks it again.
- The sketch-plane table is covered by `sketch_plane_axes_match_the_skill_reference` in the same file.
- `skills_name_only_real_tools_and_commands` checks that every tool and command the skills call exists.
- When a command, a parameter name or a behaviour that these files describe changes, update them in the same PR.
- Facts that describe current limitations, such as edge picks without a distance limit, should go once the limitation is fixed.
