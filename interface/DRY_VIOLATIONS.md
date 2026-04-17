# Interface DRY Violations

Tracked hardcoded patterns and duplication in `interface/src/` that are worth consolidating. Every entry is grounded in a grep against the current tree (not historical claims).

**Snapshot:** 2026-04-17 — post `@spacedrive/primitives` migration, Tailwind v4.

**How to use this doc:**
- Before "fixing" anything here, re-run the referenced grep. Counts drift quickly.
- Entries marked 🔴 are high-value; 🟡 are medium; ⚪ are optional.
- When you land a fix, update the entry or delete it.

---

## ⚪ Pulse-dot loading indicator — ✅ largely fixed (1 remaining occurrence)

**Pattern:**
```tsx
<div className="h-2 w-2 animate-pulse rounded-full bg-accent" />
```

**Verify:**
```bash
grep -rnE 'h-2 w-2 animate-pulse rounded-full bg-accent' interface/src/
```

**Status (as of 2026-04-17):** 25 of 26 occurrences migrated to `LoadingDot` from `@spacedrive/primitives` (`spaceui/packages/primitives/src/LoadingDot.tsx`). One remaining:

- `interface/src/routes/ChannelDetail.tsx:134`

**Fix:** Swap the last raw div for `<LoadingDot />`. After that, delete this entry from DRY_VIOLATIONS.md.

---

## 🟡 Scattered color maps (4 locations, no central tokens file)

Per-domain color mappings live as inline consts instead of semantic tokens:

| Const | File:line |
|-------|-----------|
| `TYPE_COLORS` (MemoryType → Tailwind classes) | `interface/src/routes/AgentMemories.tsx:34` |
| `EVENT_CATEGORY_COLORS` (event type → classes) | `interface/src/routes/AgentCortex.tsx:10` |
| `MEMORY_TYPE_COLORS` (array, positional) | `interface/src/routes/AgentDetail.tsx:390` |
| `platformColor()` (switch on platform name) | `interface/src/lib/format.ts:50` |

**Verify:**
```bash
grep -n "TYPE_COLORS\|EVENT_CATEGORY_COLORS\|MEMORY_TYPE_COLORS" interface/src/routes/*.tsx
grep -n "platformColor" interface/src/lib/format.ts
```

**Fix:** These are semantic color mappings, not arbitrary styling. The natural home is `@spacedrive/tokens` as named semantic tokens (e.g. `color.memory.fact`, `color.event.bulletin_generated`), consumed from interface via the existing `@spacedrive/tokens` dependency. A local `interface/src/lib/semantic-colors.ts` is acceptable if the domain is too interface-specific for the shared design system.

---

## 🟡 Generic `Field` wrapper duplicated in AgentCron.tsx

**Location:** `interface/src/routes/AgentCron.tsx:718`
```tsx
function Field({label, children}: {label: string; children: React.ReactNode}) {
  return (
    <div className="space-y-1.5">
      <label className="text-xs font-medium text-ink-dull">{label}</label>
      {children}
    </div>
  );
}
```

**Verify:**
```bash
grep -nE "^function Field" interface/src/routes/AgentCron.tsx
```

**Fix:** `@spacedrive/forms` exports typed variants (`InputField`, `SwitchField`, `SelectField`, `TextAreaField`, `RadioGroupField`) rather than a generic label-wrapper. Options:
1. Replace each `<Field>` usage with the appropriate typed variant from `@spacedrive/forms` (preferred — this keeps form semantics consistent).
2. If a plain label wrapper is genuinely needed, promote this helper to `spaceui/packages/forms/src/FieldLabel.tsx` so other pages don't re-invent it.

---

## 🟡 Grid column template duplicated in table rows

**Pattern:** `grid-cols-[80px_1fr_100px_120px_100px]` — appears twice in `interface/src/routes/AgentMemories.tsx` (lines 241 header, 292 row) and nowhere else.

**Verify:**
```bash
grep -rnE 'grid-cols-\[80px_1fr_100px_120px_100px\]' interface/src/
```

**Fix:** Low blast radius — hoist the template to a local const at the top of `AgentMemories.tsx`:
```tsx
const MEMORY_TABLE_COLS = "grid-cols-[80px_1fr_100px_120px_100px]";
```
Not worth a shared component unless a second table with the same shape appears.

---

## ⚪ `text-tiny text-ink-faint` utility combo (133 occurrences)

This pair is effectively the project's "small muted caption" style. 133 occurrences across 30+ files suggests intentional consistency, not a violation.

**Verify:**
```bash
grep -rn "text-tiny text-ink-faint" interface/src/ | wc -l
```

**Decide:**
- **Leave as-is** if the utility pair is the canonical muted-caption style. No action.
- **Promote** to a semantic utility (e.g. `text-caption-muted` in `@spacedrive/tokens`) only if the project wants to abstract typography into named roles. This is an opinion call, not drift.

---

## Reference-only: patterns considered and rejected

These were evaluated and deemed not worth consolidating:

- **Empty/error state blocks** — structurally similar but copy and CTA are always context-specific; abstracting them costs more than it saves.
- **`AnimatePresence` wrappers** — Framer Motion idioms; repetition is expected.
- **Modal/Dialog structures** — `@spacedrive/primitives` already provides the shell; per-dialog content is the whole point.
- **Pagination controls** — only a few instances and they differ in filter shape.

---

## Priority

Do these in order:

1. **Pulse-dot component** (🔴, 26 sites — highest leverage)
2. **Semantic colors in `@spacedrive/tokens`** (🟡, fixes 4 scattered maps in one move)
3. **AgentCron `Field` → `@spacedrive/forms` variants** (🟡, narrow but finishes the forms migration)
4. **Hoist grid template to const** (🟡, 2-line change)

The `text-tiny text-ink-faint` item is explicitly a decide-don't-fix.
