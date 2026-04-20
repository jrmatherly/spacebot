# OpenSpec

Structured change-management system. Every non-trivial change flows through a proposal → implementation → verification → archival cycle that captures intent before code.

## Directory Layout

```
openspec/
├── specs/            # Canonical current-state specifications
│   └── <name>/spec.md
└── changes/
    ├── <active>/     # In-progress changes (proposal, design, tasks, specs/)
    └── archive/      # Completed changes, immutable historical record
```

## Immutability Rules

- **`openspec/changes/archive/*` is immutable.** Never edit archived change files. They are the historical record of what was decided and shipped. If you find drift between an archived spec and reality, the fix is to open a **new** OpenSpec change, not to edit the archive.
- **`openspec/specs/*/spec.md`** is the canonical current-state. It is updated by merging the `specs/*/spec.md` content from an active change when that change is archived. Do not hand-edit these files outside the archive flow.

## The Four-Step Lifecycle

Invoke the appropriate skill at each step:

| Step | Skill | What it does |
|------|-------|--------------|
| 1. Propose | `/openspec-propose` | Creates `openspec/changes/<name>/{proposal.md,design.md,tasks.md,specs/*/spec.md}` |
| 2. Apply | `/openspec-apply-change` | Implements tasks in code, updates task checklist |
| 3. Verify | `/openspec-verify-change` | Confirms implementation matches the change's spec files |
| 4. Archive | `/openspec-archive-change` | Merges change's `specs/*/spec.md` into `openspec/specs/*/spec.md`, moves the change directory under `archive/` |

There is also `/openspec-explore` for thinking-partner mode before committing to a proposal.

## Common Mistakes

- **Creating a new `openspec/specs/<name>/` directory directly.** Don't. Every canonical spec comes from an archived change. Start with `/openspec-propose`.
- **Editing an archived change to "update the status".** Archives are frozen. If status shifted, note it in a follow-up change's proposal, or update `openspec/specs/*/spec.md` via a new change.
- **Hand-merging a change without archiving it.** Use `/openspec-archive-change`. Manual merges have caused retroactive-fix commits in the past (see commit `e377ac3`).
- **Forgetting to regenerate `openspec/specs/*` after archiving.** The archive skill handles this. Don't bypass it.

## Scope Relative to docs-audit

The canonical specs (`openspec/specs/*/spec.md`) can drift from reality if implementation moves without a formal change. No other skill audits this. That drift is owned by `/docs-audit`. If you find it, recommend opening an OpenSpec change rather than editing the spec directly unless the drift is purely cosmetic.
