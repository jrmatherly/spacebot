---
name: memory-writing
description: Use when creating or editing memories. Covers when to write vs skip, the eight memory types, importance scoring, association discipline, content craft, and anti-patterns that bloat the memory graph.
---

# Memory Writing

Memories are the agent's durable working context. They persist across conversations, rank against vector search, feed recall into every new message, and compound over time into the agent's effective understanding of the user, the system, and the work. A memory graph full of junk is worse than an empty one: it teaches recall to return noise, degrades search quality, and forces future branches to wade through irrelevance.

The standard is load-bearing. A memory worth writing is one that a future version of the agent — with no access to this conversation — would be genuinely helped by. The test is not "is this true?" but "will this change future behavior or unlock future decisions?"

## Before Writing

Check whether the information already exists. Run `memory_search` for the key terms before writing. Duplicate memories fragment the graph and dilute recall scoring. If a prior memory already covers the fact, edit or supersede it rather than writing a fresh one.

Ask the three gates, in order:

1. **Is this transient?** If the fact is only useful for the current conversation turn or the current work session, skip it. Memories are for what outlives context, not what restates it.
2. **Is this derivable?** If the information can be reconstructed by reading the code, running a command, or querying a known API, do not memorize it. Memorize the stable preference or decision that surrounds it, not the ephemeral value.
3. **Will this still be true next month?** If the answer is "probably not," demote the memory to a lower importance or skip it entirely. Memories decay; time-bombed facts damage trust.

If all three gates pass, write.

## Choosing a Memory Type

Eight types carry eight sets of reader expectations. Pick the one that matches how the memory will be used, not the type whose default importance happens to suit the moment.

- **identity** — Stable truths about who the user is or who the agent is. Name, role, responsibilities, long-lived affiliations. Does not decay. Reserve for facts that define the subject, not facts the subject happens to hold today.
- **preference** — What the user likes, dislikes, or prefers. Communication style, tool choices, workflow conventions. Preferences can change; importance is lower than identity but higher than fact.
- **decision** — A choice that was made, with enough context to be re-examined later. Include the alternatives considered when they matter. Decisions age well when the "why" is captured; bare "we chose X" memories rot fast.
- **goal** — Something the user or agent wants to achieve. Distinct from tasks (which are actionable and tracked): goals are directional intent that frames multiple tasks.
- **todo** — An actionable reminder that does not yet warrant a formal task. Short-lived by design; promote to a real task via `task_create` as soon as scope is concrete.
- **fact** — Something true about the world that the agent needs to remember. Use sparingly. A fact memory that could be a preference or a decision probably should be.
- **event** — Something that happened. Low importance by default because events decay fast; write them only when the happening itself is worth recalling later.
- **observation** — Something the system noticed that the user did not explicitly state. Lowest default importance. Useful for pattern accumulation across conversations.

If nothing fits cleanly, the content probably does not belong in memory.

## Content Craft

### First Sentence

The first sentence is what recall shows in previews and what ranking keys on. Lead with the subject and the fact, in the plainest form.

Good: "Jason prefers short bulleted status updates over paragraph-form summaries."
Good: "The deploy target is Talos Kubernetes; Fly.io was retired 2026-04-18."
Bad: "The user mentioned earlier today that..."
Bad: "It is worth noting that the user seems to prefer..."

### Specificity

Vague memories retrieve vaguely. Compare:

Bad: "The user doesn't like long responses."
Good: "Jason rejects responses longer than three short paragraphs in status-update contexts; prefers bullets."

The second memory gives future recall enough concrete signal to be useful. The first will match against any response-length topic and add noise.

### Standalone Readability

A memory is read with no conversation context. Include the subject name, not pronouns. Include the time or version, not "recently." A memory that only makes sense inside the conversation that produced it is a memory the agent cannot actually use.

### No Embedded Instructions

Memories are facts, not directives. "Always use bullets when replying" belongs in a preference memory that the agent's reasoning reads ("Jason prefers bullets"), not as an imperative the memory literally shouts at the LLM. The rendering layer turns preferences into behavior; the memory just records the fact.

## Importance Scoring

Importance is a 0.0 to 1.0 float that biases recall ranking. Default importances per type exist for a reason; respect them unless the specific memory genuinely differs.

- **1.0** — Identity memories. Core, non-decaying. Reserve exclusively for who-the-user-is and who-the-agent-is facts.
- **0.8 to 0.9** — Goals, decisions, todos. High-signal, longer-lived.
- **0.6 to 0.7** — Preferences, facts. The normal working memory band.
- **0.3 to 0.5** — Events, observations. Short half-life by design.

Only override the default when the memory genuinely departs from its type's typical stakes. A decision that is time-sensitive might score 0.5. An observation about a pattern the user denies might score 0.6 despite the type. Do not inflate importance to rank a memory higher; the ranker compensates and the inflation just drowns other signals.

## Associations

Associations link memories into a graph. They are the difference between a flat pile of facts and a retrievable knowledge structure.

When writing a new memory about an entity that already has memories, link to them:

- Use `related` for coarse topical proximity ("memories about the same project").
- Use `supports` or `contradicts` when the new memory agrees with or conflicts with existing ones. Contradictions are load-bearing: recall surfacing contradictory memories together lets reasoning resolve them rather than cherry-picking the highest-score hit.
- Use `refines` when the new memory is a more specific version of an existing one; the broader memory stays, the refinement narrows in.

Three to six associations is typical. Linking every memory to every other memory defeats the purpose; the graph becomes a mesh and recall degrades.

## Anti-Patterns

Things that look like memory writes but are not:

- **Chat summaries.** "We discussed the config refactor today" is an event at best, probably noise. The decisions from the discussion are worth memorizing; the meeting-happened fact is not.
- **Quoted user messages verbatim.** Paraphrase the fact the message contained. "Jason said 'I hate long emails'" is a quote; the memory is "Jason dislikes long emails."
- **Rolling aggregates.** "Jason has asked about deployment 17 times this month" is a fact that rots the moment the next question arrives. If there is signal here, the real memory is "Jason is actively focused on deployment."
- **Instructions disguised as facts.** "The agent should always respond in bullets" is a directive, not a memory. Rewrite as a preference ("Jason prefers bulleted responses") and let the rendering layer surface the behavior.
- **Every passing observation.** Observations are defensible when they accumulate into a pattern worth recalling. Writing one per turn floods the graph with low-signal content and drives recall relevance down.

## When Not to Write

Skip the memory when:

- The information is restated elsewhere in the conversation context the agent already sees (history, preamble, channel settings).
- The fact is going to expire before the next recall would surface it.
- Writing it would duplicate an existing memory rather than refine one.
- The content is a speculation or a hypothesis, not a verified fact. Memories do not carry uncertainty well; a speculative "observation" will be recalled and weighted as if it were known.
- The information is security-sensitive and would leak via recall previews.

A memory graph with two hundred carefully chosen memories is more valuable than one with two thousand that together describe every conversation.

## Editing Existing Memories

Read the full memory with `memory_get` before editing. If the new information refines an existing memory, edit rather than write. Change the content, adjust the importance if the stakes changed, and add a supersession association to any memory the edit invalidates.

When the old memory is genuinely wrong (not merely outdated), mark it forgotten via `memory_forget` rather than deleting. Forgotten memories stay in the store for audit trail and provenance but do not surface in recall.

When the old memory is outdated but still historically true, keep it as an event and write a new memory for the current truth. Link them with `refines` or `supersedes` so recall shows both and reasoning can orient.
