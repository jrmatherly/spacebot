#!/usr/bin/env bash
# graphify-rebuild.sh — thin wrapper that builds a DIRECTED graph via
# graphify's Python API. The built-in `graphify update` CLI and
# post-commit hook rebuild as undirected (Python API directed=False
# default), forfeiting the 31x query-efficiency win measured on the
# sibling talos-ai-cluster integration (523x vs 17x token reduction).
#
# This wrapper performs AST-only extraction. Semantic extraction for
# docs/papers/images requires `/graphify <path> --directed` via the
# Claude Code skill (which dispatches parallel subagents and calls the
# LLM). For a pure-AST code corpus (e.g. src/), this wrapper is
# sufficient on its own. For doc corpora, run the skill first to
# populate graphify-out/.graphify_semantic.json, then this script
# merges that semantic extraction with fresh AST + directed topology.
#
# Usage:
#   scripts/graphify-rebuild.sh <path>              # incremental
#   scripts/graphify-rebuild.sh <path> --clean      # wipe cache first
#   scripts/graphify-rebuild.sh <path> --snapshot   # write
#                                                   #   GRAPH_REPORT.md.keep
#                                                   #   with token header
#                                                   #   stripped
#
# Intended invocation via `just graphify-rebuild <path>`.

set -euo pipefail

# Resolve repo root and refuse to run outside it.
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# Argument parsing.
if [ $# -lt 1 ]; then
    echo "usage: $0 <path> [--clean] [--snapshot]" >&2
    exit 2
fi

TARGET_PATH="$1"
shift || true
CLEAN=0
SNAPSHOT=0
while [ $# -gt 0 ]; do
    case "$1" in
        --clean) CLEAN=1 ;;
        --snapshot) SNAPSHOT=1 ;;
        *) echo "unknown flag: $1" >&2; exit 2 ;;
    esac
    shift
done

if [ ! -e "$TARGET_PATH" ]; then
    echo "error: target path '$TARGET_PATH' does not exist" >&2
    exit 1
fi

# Confirm graphify is available.
GRAPHIFY_BIN="$(command -v graphify || true)"
if [ -z "$GRAPHIFY_BIN" ]; then
    echo "error: graphify not found on PATH. Install with 'pipx install graphifyy'." >&2
    echo "See .scratchpad/completed/2026-04-21-graphify-research.md for full setup." >&2
    exit 1
fi

# Resolve the Python interpreter that can actually import graphify.
# pipx isolates graphify in its own venv, so the system `python3` cannot
# `import graphify`. Read the shebang from the graphify CLI binary to
# find the right interpreter. This mirrors SKILL.md Step 1's approach.
GRAPHIFY_PY="$(head -1 "$GRAPHIFY_BIN" | sed -E 's|^#!||; s| .*||')"
if [ -z "$GRAPHIFY_PY" ] || [ ! -x "$GRAPHIFY_PY" ]; then
    echo "error: could not resolve graphify's Python interpreter from '$GRAPHIFY_BIN' shebang" >&2
    exit 1
fi
# Sanity-check: can it actually import graphify?
if ! "$GRAPHIFY_PY" -c "import graphify" 2>/dev/null; then
    echo "error: '$GRAPHIFY_PY' cannot import graphify. pipx venv may be broken." >&2
    echo "Try: pipx reinstall graphifyy" >&2
    exit 1
fi

# Clean cache if requested. Cache is append-only until manually cleared
# — changing .graphifyignore leaves stale entries for newly-excluded files.
if [ "$CLEAN" -eq 1 ]; then
    echo "graphify-rebuild: wiping graphify-out/cache/ (--clean requested)"
    rm -rf graphify-out/cache/
fi

echo "graphify-rebuild: building directed graph for '$TARGET_PATH'"
echo "graphify-rebuild: interpreter = $GRAPHIFY_PY"
"$GRAPHIFY_PY" - "$TARGET_PATH" <<'PY'
import sys, json
from pathlib import Path
from graphify.detect import detect
from graphify.extract import collect_files, extract
from graphify.build import build_from_json
from graphify.cluster import cluster, score_all
from graphify.analyze import god_nodes, surprising_connections
from graphify.report import generate
from graphify.export import to_json, to_html

target = Path(sys.argv[1])
out = Path("graphify-out")
out.mkdir(exist_ok=True)

# 1. Detect corpus.
det = detect(target)
print(f"  detected: {det['total_files']} files, ~{det['total_words']} words")

# 2. AST extraction (code files only). Semantic extraction for docs /
#    papers / images is a separate code path: invoke /graphify <path>
#    --directed via the Claude Code skill to produce
#    graphify-out/.graphify_semantic.json, then rerun this wrapper to
#    merge it with fresh AST + directed topology.
code_files = []
for f in det.get("files", {}).get("code", []):
    p = Path(f)
    code_files.extend(collect_files(p) if p.is_dir() else [p])
ast = (
    extract(code_files, cache_root=Path(".")) if code_files
    else {"nodes": [], "edges": [], "input_tokens": 0, "output_tokens": 0}
)

# 3. Merge with existing semantic extraction (from a prior skill run).
sem_path = out / ".graphify_semantic.json"
if sem_path.exists():
    sem = json.loads(sem_path.read_text())
    seen = {n["id"] for n in ast["nodes"]}
    merged_nodes = list(ast["nodes"])
    for n in sem.get("nodes", []):
        if n["id"] not in seen:
            merged_nodes.append(n); seen.add(n["id"])
    merged_edges = ast["edges"] + sem.get("edges", [])
else:
    merged_nodes = ast["nodes"]; merged_edges = ast["edges"]

# 4. Build — directed=True is the whole point of this wrapper.
G = build_from_json({"nodes": merged_nodes, "edges": merged_edges}, directed=True)

# 5. Cluster + analyze + report.
communities = cluster(G)
cohesion = score_all(G, communities)
gods = god_nodes(G)
surprises = surprising_connections(G, communities)
labels = {cid: f"Community {cid}" for cid in communities}
report_text = generate(
    G, communities, cohesion, labels, gods, surprises,
    det, {"input": 0, "output": 0}, ".",
)
(out / "GRAPH_REPORT.md").write_text(report_text)
to_json(G, communities, str(out / "graph.json"))
to_html(G, communities, str(out / "graph.html"), labels)

print(
    f"  built: {G.number_of_nodes()} nodes, {G.number_of_edges()} edges, "
    f"{len(communities)} communities (directed)"
)
PY

# Optional snapshot: strip token-count header lines that regenerate on
# every build (otherwise every commit would churn the diff). We match
# the broadest plausible set of header forms; if graphify's report
# format changes, update this regex.
if [ "$SNAPSHOT" -eq 1 ] && [ -f graphify-out/GRAPH_REPORT.md ]; then
    echo "graphify-rebuild: writing graphify-out/GRAPH_REPORT.md.keep (token header stripped)"
    sed -E '/^(This run|Input tokens?|Output tokens?|Token cost|Total tokens?):/Id' \
        graphify-out/GRAPH_REPORT.md > graphify-out/GRAPH_REPORT.md.keep
fi

echo "graphify-rebuild: done. Open graphify-out/graph.html to view, or run"
echo "  graphify query \"<your question>\"   # BFS traversal of graph.json"
