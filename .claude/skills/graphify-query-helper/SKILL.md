---
name: graphify-query-helper
description: Query the local graphify knowledge graph to answer semantic cross-document questions. Wraps `just graphify-query` with synthesis and citation. Use when a question spans design docs, RFCs, or screenshots that a single-file read cannot answer, or when the user explicitly asks about "what connects X to Y", "why did we decide Z", or "which doc explains W". Requires a pre-built graph at `graphify-out/graph.json` — if missing, stops and tells the user to run `just graphify-rebuild docs/design-docs/` first.
---

# /graphify-query-helper

Thin orchestrator around the graphify CLI. Graphify itself returns a BFS traversal of the graph as raw node/edge dumps. That output is hard to read and provides no citations. This skill wraps a query with:

1. Graph-existence check (fail-fast if `graphify-out/graph.json` is missing).
2. Invocation of `just graphify-query` with the user's question.
3. Synthesis of the traversal output into a prose answer with `source_file:source_location` citations drawn from the graph's node metadata.
4. A transparency line about graph freshness — the graph's build timestamp, so the user knows whether the answer reflects current docs or a stale snapshot.

## When to use

Triggers:
- User asks a question that spans multiple design docs ("why did we make X decision related to Y?").
- User asks for a connection path ("how does the Entra rollout relate to the Spacedrive integration?").
- User asks for an explanation of a concept that appears in multiple places ("explain `AuthRejectReason`").

Not for:
- Structural Rust code questions — use `code-review-graph` MCP tools instead (it has the full AST; graphify's AST pass covers code but `code-review-graph` is faster and fresher).
- Questions the user could answer by reading one file — don't over-engineer.
- A fresh workspace where no graph has been built. Say so, don't fabricate an answer.

## Behavior

### Step 1 — verify graph exists and is fresh enough

```bash
cd /Users/jason/dev/spacebot
if [ ! -f graphify-out/graph.json ]; then
    echo "No graph at graphify-out/graph.json."
    echo "Run: just graphify-rebuild docs/design-docs/"
    exit 1
fi
# Report freshness
GRAPH_AGE_DAYS=$(( ($(date +%s) - $(stat -f %m graphify-out/graph.json)) / 86400 ))
echo "Graph age: ${GRAPH_AGE_DAYS} day(s)"
```

If the graph is more than 30 days old, lead the answer with a staleness caveat: *"Note: the graph was built N days ago — facts about recent commits may not appear."*

### Step 2 — pick traversal mode based on question shape

| Question shape | Mode |
|----------------|------|
| "What connects X to Y?" | `just graphify-query "X Y"` + `graphify path "X" "Y"` |
| "Explain X" | `graphify explain "X"` (single-node expansion) |
| "Why did we do Z?" | `just graphify-query "why Z"` — BFS finds rationale nodes |
| "How does X work?" | `just graphify-query "X how"` — BFS finds related concepts |

### Step 3 — run the query

```bash
just graphify-query "<question>" 2>&1 | tee /tmp/graphify-query-raw.txt
```

Capture both stdout and stderr. If the output contains `No matching nodes found`, do NOT fabricate an answer from training data — say so explicitly: *"Graphify returned no matches for this query. The graph may not cover this topic, or the question's keywords don't match any node labels."*

### Step 4 — synthesize with citations

For each node/edge in the raw output, extract:
- `source_file` — always cite this when presenting a fact.
- `source_location` — include line numbers when available.
- `confidence` tag — if `AMBIGUOUS`, note uncertainty in the answer.

Answer template:

```
Based on the graph built <TIMESTAMP> over <CORPUS> (<NODE_COUNT> nodes, <EDGE_COUNT> edges):

<SYNTHESIZED PROSE ANSWER>

Sources:
- <node label> (<source_file>:<source_location>) — <one-line summary>
- <node label> (<source_file>) — <one-line summary>
```

If the synthesis pulls from INFERRED or AMBIGUOUS edges, flag it: *"The graph infers this connection rather than finding it explicitly in the source — verify before citing."*

### Step 5 — save the Q&A back to the graph's memory

Graphify has a feedback-loop mechanism for memorializing good answers:

```bash
graphify save-result \
    --question "<exact question the user asked>" \
    --answer "<your full synthesized answer>" \
    --type query \
    --nodes <node labels cited>
```

This writes to `graphify-out/memory/` and makes the Q&A surface as a node in the next `--update` pass. Only do this when the answer is high-confidence. For AMBIGUOUS or "no matches" paths, skip — don't pollute the memory with low-signal entries.

## Example invocations

### User asks a cross-doc question

```
User: "Why did we rename auth.rs to anthropic_oauth.rs?"

Skill:
  1. Verifies graph exists (freshness: 2 days).
  2. Runs: just graphify-query "rename auth.rs anthropic_oauth"
  3. BFS traversal returns nodes: {auth_rs_rename, entra_phase_1, anthropic_oauth_pkce}
     with edges from entra-app-registrations.md citing the rename.
  4. Synthesizes:
     "The rename happened during Entra Phase 1 (PR #82, 2026-04-20).
      src/auth.rs previously held the Anthropic OAuth PKCE flow. Phase 1
      introduced src/auth/ as a module tree for Entra JWT validation,
      which needed the auth namespace. The OAuth PKCE code was moved to
      src/anthropic_oauth.rs to free the path, with 10 call sites migrated.
      Sources:
      - entra-app-registrations.md:42 (EXTRACTED)
      - entra-phase-1 node (INFERRED, confidence 0.8)"
  5. graphify save-result to memorize this answer.
```

### User asks a question the graph can't answer

```
User: "What's the latest on the Entra Phase 5 design?"

Skill:
  1. Verifies graph exists (freshness: 5 days).
  2. Runs: just graphify-query "Entra Phase 5"
  3. Output: "No matching nodes found."
  4. Responds:
     "The graph has nodes for Phase 1-3 but no match for Phase 5.
      Either the design doc hasn't been written yet, or the graph
      needs rebuilding to pick up recent design work. Try:
      ls docs/design-docs/ | grep -i phase-5
      If a file exists, run: just graphify-rebuild docs/design-docs/"
```

## Related

- `spacebot-dev` skill — use for Rust architectural questions.
- `spacedrive-dev` skill — use for Spacedrive-integration questions.
- `code-review-graph` MCP tools — use for structural code queries (faster than graphify for code-only questions).
- `.scratchpad/completed/2026-04-21-graphify-research.md` — full context on graphify itself, including why this skill exists.

## Honesty rules

- Never invent a source citation. If the graph doesn't provide a `source_location`, omit the citation rather than fabricate one.
- Always report graph freshness. A 30-day-old graph answering a question about yesterday's commit is misleading.
- Never claim an INFERRED or AMBIGUOUS edge is a definitive connection — flag the uncertainty.
- If `just graphify-query` returns "No matching nodes found", say so. Do not fall back to training-data answers about the codebase.
