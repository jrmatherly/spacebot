# Preset Authoring

> **Status:** Living guide. Written against the nine shipped presets under `presets/` and the factory integration documented in `docs/design-docs/agent-factory.md`. Update alongside any change to the preset file contract.

Spacebot agents are created from presets. A preset is a short, opinionated package that sets an agent's personality, role, and scope before the factory runs any per-user tailoring. It ships as a directory of markdown plus a TOML manifest embedded in the binary. This guide covers what a preset is, what goes in each file, how voice rules differ across files, and how to verify a new preset loads.

## Scope

**In scope.** The four-file preset contract (`IDENTITY.md`, `ROLE.md`, `SOUL.md`, `meta.toml`), voice rules per file, the `meta.toml` schema as enforced by `src/factory/presets.rs`, the `include_dir!` embedding mechanism, and the test pattern used to verify a new preset loads.

**Out of scope.** The factory flow itself (`docs/design-docs/agent-factory.md`), the 9 existing preset archetypes' domain specifics, and the runtime tailoring that happens after factory creation (per-user identity, tool gating, model routing).

## Ground truth

| Fact | Source |
|---|---|
| Preset directory | `presets/<preset-id>/` |
| Required files | `IDENTITY.md`, `ROLE.md`, `SOUL.md`, `meta.toml` |
| Embedding | `include_dir!` macro in `src/factory/presets.rs` |
| Nine shipped presets | `community-manager`, `content-writer`, `customer-support`, `engineering-assistant`, `executive-assistant`, `main-agent`, `project-manager`, `research-analyst`, `sales-bdr` |
| Model routing | Intentionally excluded from `meta.toml`. Model selection happens during the factory conversation when the user's available providers are known. |
| Registry API | `PresetRegistry::list() -> Vec<PresetMeta>`, `PresetRegistry::load(id) -> Option<Preset>` |
| Three builtin skills with authoring examples | `skills/builtin/memory-writing`, `skills/builtin/task-triage`, `skills/builtin/wiki-writing` |

## The four files

Every preset directory contains exactly four files. Missing any one breaks preset loading.

### IDENTITY.md

What the agent **is**. Who the user should expect when they spawn this preset. Typically 15 to 25 lines. One top-level `# Identity` heading, a one-paragraph opener naming the role, followed by `## What You Do` (bulleted capabilities) and `## Scope` (explicit limits).

- Voice: second-person to the agent ("You are an engineering assistant agent..."). This becomes part of the system prompt the LLM reads in first person.
- Content rule: describe the role, not the implementation. "You review pull requests" is an IDENTITY fact. "You use the `github_review` tool" is not; that belongs in ROLE.
- The `## Scope` block is load-bearing. It tells the LLM what NOT to do. "You don't deploy to production" or "You don't make architectural decisions unilaterally" are the kinds of limits that prevent overreach when the agent has broad tool access.

Good lead: "You are a community manager agent. You engage with members, moderate discussions, and keep the conversation constructive across Discord, Slack, and other platforms where the community lives."

Bad lead: "This preset is for community management tasks." (Describes the preset, not the agent. The LLM reads the text as its own identity; self-reference in the third person is confusing.)

### ROLE.md

How the agent **works**. Concrete procedures, checklists, decision rules. Typically 30 to 60 lines. Top-level `# Role` heading, then one or more `## <Procedure>` sections.

- Voice: second-person imperative. "Read the PR description before looking at code." "Batch your review."
- Content rule: capture the operational discipline a human expert in this role would apply. If there is a standard way to do the work, name it. If the role has a checklist, write the checklist.
- Tool mentions are allowed here, but only when the tool is the canonical way to perform a step. "Use `task_create` when scope is concrete" is load-bearing; "The `task_create` tool accepts a title parameter" is tool documentation that belongs in `prompts/en/tools/task_create_description.md.j2`, not ROLE.

Good section: a numbered review procedure, labeled steps, each with one sentence of guidance.

Bad section: a list of "things to keep in mind." Vague guidance trains the LLM to ignore ROLE at runtime.

### SOUL.md

How the agent **speaks**. Personality, voice, default dispositions. Typically 25 to 40 lines. Top-level `# Soul`, `## Personality`, `## Voice`.

- Voice: second-person declarative. "You are direct and precise." "You back opinions with reasoning, not dogma."
- Content rule: capture the posture the agent takes toward its work and its interlocutor. A tone an engineer would recognize in a colleague. Specific enough to be felt; not so specific that every reply sounds templated.
- The `## Voice` section usually has 3-6 bullets: preferred register, preferred format, things not to do (no compliment sandwiches, no padded responses, etc.).

Good SOUL has opinions. "If the code is good, you say so briefly. If there's a problem, you explain what and why, and suggest a fix. No compliment sandwiches."

Bad SOUL is neutral. "You communicate professionally and help users accomplish their goals." This could be any agent; the LLM learns no differentiating behavior.

### meta.toml

Machine-readable metadata. Parsed into `PresetMeta` at load time. Five required top-level keys plus a `[defaults]` table.

```toml
id = "engineering-assistant"                # kebab-case, must match directory name
name = "Engineering Assistant"              # human-readable display name
description = "Code review, architecture guidance, PR management, technical documentation, and developer support."
icon = "code"                                # Lucide icon name used by the web UI
tags = ["engineering", "code-review", "architecture", "github", "technical"]

[defaults]
max_concurrent_workers = 5                   # How many workers can run simultaneously
max_turns = 8                                # Max LLM turns per channel conversation
```

- `id` must be valid kebab-case and must match the parent directory name. Mismatch is caught at factory load time.
- `description` is user-facing in the preset picker UI. Keep it under 120 characters.
- `tags` feed search and filtering. 4 to 6 tags is typical; the first tag usually restates the primary domain.
- `[defaults]` values flow through to the created agent's runtime config. `max_concurrent_workers` and `max_turns` are the load-bearing two today; new keys should be documented in `docs/design-docs/agent-factory.md` before being added here.
- **Model routing is intentionally absent.** Per the factory design, presets are provider-agnostic. Model selection happens during the factory conversation. Do not add a `model` or `provider` key.

## Voice rules across files

The three markdown files set the agent's "self-text" that the LLM reads as its own identity. Each file plays a different structural role, and the voice rule differs:

| File | Voice | Purpose |
|---|---|---|
| `IDENTITY.md` | second-person declarative ("You are...") | Who the agent is |
| `ROLE.md` | second-person imperative ("Do X. Check Y.") | How the agent works |
| `SOUL.md` | second-person declarative ("You think...") | How the agent speaks |

Mixing voices breaks the LLM's self-modeling. IDENTITY that uses imperatives ("Review pull requests") reads like a task list, not an identity. ROLE that uses declaratives ("You review pull requests") reads like identity restated, losing procedural force.

The `writing-guide.md` em-dash rule does **not** apply to preset files. Presets are consumed by the LLM at inference time, matching the precedent set by the `prompts/**/*.md.j2` exemption and by the builtin skills (`skills/builtin/*/SKILL.md`). If you write em-dashes in prose inside a preset file, that is intentional.

## Anti-patterns

Things that look like preset content but are not:

- **USER.md or user-context content.** The 2026 factory design deprecated USER.md; per-user context is owned by the factory conversation and the agent's memory graph at runtime, not shipped in the preset. A preset that includes user-specific facts ("the user prefers bulleted responses") leaks one instance's context into every other instance that spawns the preset.
- **Tool documentation.** If a preset describes tool parameters or usage, that content belongs in `prompts/en/tools/<tool>_description.md.j2`. A preset mentions a tool by name when the tool is the canonical way to perform a step; it does not document the tool.
- **Every-preset filler.** Lines like "You are professional and helpful" or "You communicate clearly" apply to every preset and therefore differentiate nothing. Strip them; what distinguishes this preset from the eight others is the content worth keeping.
- **Implementation bleed.** A preset that mentions `src/factory/presets.rs` or `include_dir!` is leaking implementation detail into the agent's self-model. Save that for the design doc.
- **Aspirational capabilities.** Only list capabilities the agent can actually perform with its shipped tool surface. An engineering-assistant preset that lists "deploys to production" when the agent has no deploy tool misleads the LLM into asserting capabilities it cannot execute.

## Adding a new preset

1. Create `presets/<new-id>/` with the four required files.
2. Author `IDENTITY.md`, `ROLE.md`, `SOUL.md` following the voice-rule table above. Match the length ranges (15-25 / 30-60 / 25-40 lines respectively).
3. Author `meta.toml` with `id` matching the directory name.
4. Add `<new-id>` to any preset-listing fixtures used by factory tests. Run `cargo test --lib factory::presets` to verify the preset loads. A failing `PresetRegistry::load` signals a contract violation (missing file, malformed TOML, or `id` mismatch).
5. Update `docs/design-docs/agent-factory.md` Phase 1 preset table to include the new entry, and update the preset-count claim (currently "Nine preset archetypes" at `agent-factory.md:177`).
6. Update CHANGELOG.md under `Added` with the new preset, its description, and its intended use case.

## Verifying a preset loads

The factory's preset loading is tested in `src/factory/presets.rs`. A new preset should:

- Appear in `PresetRegistry::list()` output.
- Load fully via `PresetRegistry::load("<new-id>")` with non-empty `soul`, `identity`, and `role` fields.
- Parse `meta.toml` without error.
- Pass the preset-count assertion in whatever test asserts "the shipped set is exactly N presets" by incrementing N alongside adding the preset.

If a test asserts "exactly N presets" and you bump N, also bump the count claim in `agent-factory.md:177` and in any other design doc that names the total.

## Related skills

- `skills/builtin/memory-writing/SKILL.md` — how the agent writes memories. A preset's IDENTITY / ROLE should not duplicate this guidance; reference it implicitly through the agent's behavior.
- `skills/builtin/task-triage/SKILL.md` — how the agent handles tasks. Same principle; a preset does not restate skill content.
- `skills/builtin/wiki-writing/SKILL.md` — how the agent writes wiki entries. Reference, do not restate.

The three builtin skills plus the prompt-authoring guidance in `docs/design-docs/agent-factory.md` collectively cover the reusable behavior presets can assume. A new preset that tries to encode one of these domains inline is authoring a worse copy of existing content.
