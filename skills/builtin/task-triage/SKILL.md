---
name: task-triage
description: Use when creating, updating, or shepherding tasks through the lifecycle. Covers when to elevate vs decompose, priority signals, status discipline, the owner-vs-assigned split, and the global-task vs per-agent distinction.
---

# Task Triage

Tasks are the structural backbone of agent autonomy. A conversation without tasks is a conversation that ends when the channel goes idle; a conversation with tasks survives restart, survives handoff, and drives work that spans multiple agents and days. Poorly triaged tasks break that structure: backlogs swell with noise, priorities lose meaning, workers pick up items that should have been handled inline, and the global task board becomes a dumping ground nobody trusts.

The standard is that every task earns its place. The test is not "can this be framed as a task?" but "will tracking this as a task unblock future work, compared to handling it inline?"

## Before Creating

Check whether an existing task covers the work. Run `task_list` with the relevant agent or status filter before creating a new task. Overlapping tasks fragment ownership; deduplicating them later costs more than a prior search.

Ask the three gates, in order:

1. **Is this already happening right now?** If the work is in the current turn and will finish before the next user message, do it. A reply that says "I'll do X" and then does X does not need a task. A reply that says "I'll do X next Tuesday" does.
2. **Does this require a worker or a branch?** If the work is heavy enough to warrant spawning a sub-process, task tracking carries the branch/worker through to completion and lets the user see status. If it is light, keep it inline.
3. **Will someone other than the current agent need to pick this up?** Tasks are the coordination mechanism between agents. If the work stays with the current agent and finishes in-session, a task is overhead; if it crosses an agent boundary, a task is essential.

If at least one gate passes, create the task.

## Owner vs Assigned

Every task has two agent references. They usually differ.

- **owner_agent_id** is who is accountable for the work getting done. The owner answers "why hasn't this finished?" The owner does not necessarily execute the work.
- **assigned_agent_id** is who is currently doing the work. Assignment is reassignable; ownership is stable.

Default both to the current agent if the work stays local. Split them when the work needs to be handed off. A sales agent can own a follow-up task that the engineering assistant is assigned to execute; the sales agent tracks completion, the engineering assistant does the work.

When handing a task to a different agent, use `assigned_agent_id` reassignment, not task deletion + recreation. Preserving the task ID keeps history, audit trail, and any associated memory linkages intact.

## Choosing Status

Five statuses form the lifecycle. Each transition is intentional; skipping states loses signal.

- **pending_approval** — The task is proposed but not yet greenlit. Use when the user should confirm before work starts. The `approved_at` and `approved_by` fields capture who greenlit the task.
- **backlog** — The task exists but is not ready to start. Waiting on a dependency, waiting on information, or explicitly deferred. Use `backlog` for work that is real but not now.
- **ready** — The task has a clear definition of done and can be picked up. A worker or assigned agent can start immediately.
- **in_progress** — A worker or agent has started. The `worker_id` field links the task to the running process.
- **done** — Work is complete. The `completed_at` field captures when. Do not use `done` as "abandoned"; incomplete tasks stay in their state and get an updated status or a comment explaining the deferral.

The backlog → ready transition is the most important. A ready task is a task that a fresh worker can execute without asking clarifying questions. If the task cannot pass that bar, it stays in backlog until it can. A task that sits in `ready` for weeks is a sign that the ready criteria were wrong.

## Priority Signals

Four priorities exist. Pick based on the cost of delay, not on the agent's current enthusiasm for the topic.

- **critical** — Blocks user-visible functionality or active work. A critical task interrupts the current agent's other work; it is handled before lower-priority items even if those are older.
- **high** — Should land in the current session or soon after. The user is waiting on it; other tasks should not accumulate in front of it.
- **medium** — Normal working priority. Most tasks sit here. Handled in order with reasonable responsiveness.
- **low** — Valuable but not urgent. Will be picked up when the backlog clears. Low tasks that sit for months are candidates for conversion to a goal memory and task deletion.

Inflation kills the signal. When every task is critical, the priority field tells the worker nothing. Reserve critical for work that genuinely interrupts; demote aggressively when the original urgency has passed.

## Decomposition

When to decompose a task into subtasks:

- The work has distinct stages with independent definitions of done.
- The work benefits from visible checkpoint progress the user can see.
- The parent task would otherwise sit in `in_progress` for days with no visible state change.

When not to decompose:

- The subtasks would all be "step N of the same sequential procedure." Use the task's `description` to document steps; keep the task atomic.
- The subtasks are smaller than a few minutes of work each. Granularity that fine is noise, not structure.
- The parent task is itself small enough to complete as a single unit. Decomposing a thirty-minute task into five six-minute subtasks creates bureaucracy without progress visibility.

Subtasks are a flat list per task. Nested subtasks (subtasks with their own subtasks) are an anti-pattern; if the work is that structured, the outer subtasks should be promoted to their own tasks.

## Elevating to Global

Tasks live in an instance-wide store with globally unique `task_number` identifiers. Every task is "global" in that sense. What differs is visibility and the expected consumer.

- A task owned and assigned to a single agent is a local task in practice; other agents see it only if they query with the relevant `agent_id` filter.
- A task owned by one agent but assigned to another (or created by one agent for another) is a handoff; both sides track it.
- A task in the instance-wide UI (`Workers` or `Tasks` tab) is visible to every agent at that instance.

Elevate a task when:

- Multiple agents need to see the same outstanding work.
- A user wants a unified view across agent conversations ("what is my team working on?").
- The task outlives any single conversation and needs to be addressable from any channel.

Do not elevate personal task state that is only relevant to one agent's immediate work. Elevation adds noise to other agents' task queries without giving them actionable items.

## The source_memory_id Link

Tasks can reference the memory that motivated them. Populate `source_memory_id` when:

- The task was created from a user statement that is itself a memory (e.g., a preference or a decision that implies action).
- Future recall of the source memory should surface the derived task.
- Auditing the chain from intent to execution matters.

Do not populate it when the task has no memory ancestor (task created from a direct user request in the current turn, or from an automated trigger). A dangling or wrong `source_memory_id` is worse than a missing one; it misleads future recall.

## Status Discipline

Updating status is not optional. Every state transition produces a signal the user and other agents rely on:

- Mark `in_progress` the moment a worker starts, not after. An unmarked `in_progress` task looks idle.
- Mark `done` the moment the work is complete, not at end-of-session. A batch of status updates at the end loses the timing information the `updated_at` field encodes.
- When a task is blocked, do not silently leave it in `ready` or `in_progress`. Move it back to `backlog` and update `metadata` or `description` to record the blocker.
- When a task is no longer relevant (scope change, user cancellation), use a status transition that captures that — do not delete. Deleted tasks lose audit trail.

## Anti-Patterns

Things that look like task creation but are not:

- **"I will remember to do X."** That is a memory (todo type), not a task. Promote to a real task when scope is concrete.
- **"Agent A should do Y."** That is a directive. If agent A will actually pick it up, create the task assigned to A. If agent A will not see it unless told, send a cross-channel message instead.
- **"Let me track that for you."** Without a concrete deliverable, a tracking task is a procrastination dressed as structure. Either define the deliverable or skip.
- **Every user request.** Most messages are answered in the turn. A task for "reply to user's question" is silly overhead; the reply itself is the work.
- **Aspirational roadmap items.** Goals belong in goal memories. A task with no "ready to start" criteria is a goal pretending to be a task.

## When Not to Create

Skip the task when:

- The work finishes in the current turn.
- No agent will pick it up; it is wishful thinking dressed as a task.
- An existing task covers the same work; update that one instead.
- The work is inherently ongoing (maintenance, monitoring); use recurring cron or a background process, not a task.

A task board with thirty well-curated tasks is more valuable than one with three hundred stale proposals.

## Editing Existing Tasks

Read the full task with `task_get` before editing. Understand the current status, owner, and context.

When adding context, put it in `description` or `metadata`, not by renaming `title`. Titles are searchable and appear in task lists; rewriting the title as the scope changes makes history hard to follow.

When a task needs to be split, create the new tasks and then mark the original `done` with an updated description explaining the split. Do not leave the original in `in_progress` while children proceed; that double-counts the work.

When a task needs to be closed without completion, use status `done` and explain in `description` or a comment why. "Abandoned" or "cancelled" are not statuses; `done` plus a note captures the outcome with the history intact.
