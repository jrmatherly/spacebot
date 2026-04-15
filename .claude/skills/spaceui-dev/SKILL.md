---
name: spaceui-dev
description: Specialized SpaceUI component library skill for AI coding assistants. Covers the 6-package monorepo (@spacedrive/tokens, primitives, forms, ai, explorer, icons), 41+ UI primitives, 15 AI agent components, design token system (50+ CSS variables), Tailwind v4 configuration, form integration (React Hook Form + Zod), semantic color system, component variants (CVA), and consumer integration patterns. Use when working on UI components, styling with SpaceUI tokens, building AI chat interfaces, task management UI, or integrating SpaceUI into consuming apps.
---

# SpaceUI Development Guide

SpaceUI (`@spacedrive` scope) is the shared design system for the Spacedrive ecosystem. It provides React components consumed by both the Spacedrive app and the Spacebot portal. Source lives in `.scratchpad/spaceui/`.

**Stats:** 124 source files, 100+ React components, 6 npm packages, all at version 0.2.3.

## Monorepo Structure

```
packages/
  tokens/      @spacedrive/tokens      Pure CSS design tokens for Tailwind v4
  primitives/  @spacedrive/primitives  41+ base UI components
  forms/       @spacedrive/forms       8 react-hook-form field wrappers
  ai/          @spacedrive/ai          15 AI agent components
  explorer/    @spacedrive/explorer    4 file management components
  icons/       @spacedrive/icons       File type icons + resolution utilities
```

**Tooling:** Bun (never npm/pnpm/yarn), tsup (ESM), Turbo, TypeScript strict, Storybook 8.6, Changesets, React 18/19.

## Design Tokens (@spacedrive/tokens)

CSS-first tokens for Tailwind v4. No JS config file needed. Imported as pure CSS.

### Color System (semantic, never raw Tailwind colors)

| Category | Tokens | Usage |
|----------|--------|-------|
| **Accent** | `accent`, `accent-faint`, `accent-deep` | Primary actions, links |
| **Ink (text)** | `ink`, `ink-dull`, `ink-faint` | Body text, secondary, tertiary |
| **App surfaces** | `app`, `app-box`, `app-dark-box`, `app-overlay`, `app-input`, `app-hover`, `app-selected`, `app-line`, `app-divider`, `app-button`, `app-frame`, `app-shade` | Main content area |
| **Sidebar** | `sidebar`, `sidebar-box`, `sidebar-line`, `sidebar-ink`, `sidebar-ink-dull`, `sidebar-selected`, `sidebar-button`, `sidebar-divider` | Navigation sidebar |
| **Menu** | `menu`, `menu-line`, `menu-ink`, `menu-faint`, `menu-hover`, `menu-selected`, `menu-shade` | Dropdowns, context menus |
| **Status** | `status-success`, `status-warning`, `status-error`, `status-info` | State indicators |

All support opacity modifiers: `bg-accent/50`, `bg-sidebar/65`.

### 7 Themes

| Theme | CSS Class | Character |
|-------|-----------|-----------|
| Dark (default) | `.dark` | Blue-purple hue 235 |
| Light | `.light` / `.vanilla-theme` | Light backgrounds |
| Midnight | `.midnight-theme` | Deep blue, high saturation |
| Noir | `.noir-theme` | Pure grayscale |
| Slate | `.slate-theme` | Cool blue-gray |
| Nord | `.nord-theme` | Nordic blue |
| Mocha | `.mocha-theme` | Warm brown |

### Non-Color Tokens
- **Radius:** `radius-window` (10px), `radius-lg` (8px), `radius-md` (6px)
- **Fonts:** `font-sans` (Inter), `font-mono` (JetBrains Mono)
- **Font sizes:** `text-tiny` (0.7rem) through `text-7xl`

## Tailwind v4 Configuration

CSS-first approach. No `tailwind.config.js`.

### Consumer Setup
```css
@import "tailwindcss";
@import "@spacedrive/tokens/src/css/theme.css";
@import "@spacedrive/tokens/src/css/base.css";
@source "../../spaceui/packages/primitives/src";
@source "../../spaceui/packages/ai/src";
```

### Key v4 Changes from v3
- `@import "tailwindcss"` replaces `@tailwind base/components/utilities`
- `@theme {}` replaces JS config
- `@source` replaces `content` array
- `shadow-sm` -> `shadow-xs`, `shadow` -> `shadow-sm`
- `rounded-sm` -> `rounded-xs`, `rounded` -> `rounded-sm`
- `ring` is now 1px (was 3px), use `ring-3` for old behavior
- No `bg-opacity-*`, use `bg-black/50` instead

## Primitives (@spacedrive/primitives)

41+ components built on Radix UI, Headless UI, CVA, and Framer Motion.

### Key Components

**Button** — `variant`: default/subtle/outline/dotted/gray/accent/colored/bare. `size`: icon/xs/sm/md/lg. `rounding`: none/left/right/both/full. Supports `href` for links, `loading` prop.

**Input** — `variant`: default/transparent. `size`: xs(25px)/sm(30px)/md(36px)/lg(42px)/xl(48px). Props: `icon`, `iconPosition`, `right` (slot), `error`.

**Dialog** — Imperative `dialogManager.create()` + `useDialog` hook + `Dialogs` renderer. @react-spring/web animated. Form integration. Sub-components: Root/Trigger/Portal/Close/Overlay/Content/Header/Footer/Title/Description.

**Popover** — `usePopover()` hook. Composable: Root/Trigger/Content/Anchor/Close/Portal.

**Select** — CVA variants: default, sizes sm/md/lg. Composable: Root/Group/Value/Trigger/Content/Label/Item/Separator.

**Toast** — Sonner-based. `toast.info()`, `toast.success()`, `toast.error()`, `toast.warning()`. Promise toasts, action buttons.

**DropdownMenu / ContextMenu** — Full Radix composable pattern.

**Other:** CheckBox, Switch, Slider, RadioGroup, Tabs, Tooltip (with `Kbd`), Loader, Divider, ProgressBar, CircularProgress, SearchBar, Shortcut, Card (composable), ShinyToggle, InfoBanner, Resizable, Badge, Banner, ToggleGroup, Collapsible, NumberStepper, FilterButton, OptionList.

## Forms (@spacedrive/forms)

React Hook Form + Zod wrappers around primitives.

```typescript
import { Form, InputField, SelectField, CheckboxField, SwitchField } from '@spacedrive/forms';
```

Pattern: `FormItem > FormLabel > FormControl > Primitive > FormDescription > FormMessage`

Fields: `InputField`, `TextAreaField`, `SelectField`, `CheckboxField`, `RadioGroupField`, `SwitchField`.

Utilities: `Form` (FormProvider), `FormField` (Controller), `useFormField`.

## AI Components (@spacedrive/ai)

15 components for AI agent interfaces.

### Types
```typescript
type ToolCallStatus = 'running' | 'completed' | 'error';
type TaskStatus = "pending_approval" | "backlog" | "ready" | "in_progress" | "done";
type TaskPriority = "critical" | "high" | "medium" | "low";
interface ToolCallPair { id, name, argsRaw, args, resultRaw, result, status, title? }
interface Task { id, task_number, title, description?, status, priority, owner_agent_id, assigned_agent_id, subtasks, metadata, worker_id?, created_by, created_at, updated_at, completed_at? }
interface ModelOption { id, name, provider, context_window?, capabilities? }
```

### Components

| Component | Purpose |
|-----------|---------|
| **ToolCall** | Tool invocation display with per-tool renderers (shell, browser, file ops). ANSI support. |
| **Markdown** | Agent response renderer (react-markdown + remark-gfm + rehype-raw) |
| **MessageBubble** | Chat message container |
| **InlineWorkerCard** | Collapsible worker card with transcript |
| **InlineBranchCard** | Branch task display |
| **ModelSelector** | Searchable model picker with provider grouping |
| **ChatComposer** | Expanding textarea with project pill, model selector, voice button |
| **TaskRow** | Grid row with status/priority icons, SPC-N number, subtask progress |
| **TaskList** | List of TaskRow components |
| **TaskDetail** | Full task detail view |
| **TaskCreateForm** | Task creation form |
| **TaskStatusIcon** | Status icon by task status |
| **TaskPriorityIcon** | Priority icon by task priority |

### Helpers
- `pairTranscriptSteps(steps)` — Pairs tool_call with tool_result by call_id
- `tryParseJson(text)` — Safe JSON parse
- `isErrorResult(text, parsed)` — Detects error results
- `TASK_STATUS_ORDER` — Display ordering array
- `TASK_STATUS_LABEL` / `TASK_PRIORITY_LABEL` — Human-readable labels

## Explorer (@spacedrive/explorer)

4 file management components (trimmed from 14 — complex app-level views removed).

| Component | Props | Purpose |
|-----------|-------|---------|
| **TagPill** | `color`, `size` (xs/sm/md), `onRemove?` | Colored tag pill with dot indicator |
| **RenameInput** | `name`, `extension?`, `onSave`, `onCancel` | Inline file rename editing |
| **FileThumb** | `iconSrc`, `thumbnailSrc?`, `size?` | Layered file thumbnail (icon + overlay + badge) |
| **GridItem** | `name`, `extension?`, `selected?`, `tags?`, `thumb` | Grid file item with thumbnail and tag dots |

## Icons (@spacedrive/icons)

198 PNG icons for file types. Light/dark variants. 20px small variants.

```typescript
import { getIcon, getBeardedIcon, getIcon20, iconNames } from '@spacedrive/icons/util';
// getIcon(kind, isDark?, extension?, isDir?) -> IconAsset
// getBeardedIcon(extension?, fileName?) -> badge icon name
```

SVG directories: `svgs/brands/` (brand icons), `svgs/ext/` (extension badges with `icons.json` mapping).

## Integration Patterns

### Install from npm
```bash
bun add @spacedrive/tokens @spacedrive/primitives @spacedrive/forms @spacedrive/ai @spacedrive/explorer
```

### Vite (local dev with HMR)
```ts
const spaceui = path.resolve(__dirname, '../spaceui/packages');
resolve: {
  dedupe: ['react', 'react-dom'],
  alias: [
    { find: '@spacedrive/primitives', replacement: `${spaceui}/primitives/src/index.ts` },
    { find: '@spacedrive/ai', replacement: `${spaceui}/ai/src/index.ts` },
    // ... same pattern for forms, explorer, tokens
  ]
},
optimizeDeps: { exclude: ['@spacedrive/tokens', '@spacedrive/primitives', ...] },
server: { fs: { allow: ['..', '../spaceui'] } }
```

### Next.js
```ts
transpilePackages: ['@spacedrive/primitives', '@spacedrive/ai', '@spacedrive/forms', '@spacedrive/explorer']
```

### React Native / NativeWind
```js
const sharedColors = require('@spacedrive/tokens/raw-colors');
module.exports = { theme: { extend: { colors: { accent: sharedColors.accent } } } };
```

## Common Import Paths

```typescript
// Tokens (CSS imports)
import '@spacedrive/tokens/theme';
import '@spacedrive/tokens/css';
import '@spacedrive/tokens/css/themes/midnight';

// Primitives
import { Button, Input, Dialog, Select, Tooltip, Card } from '@spacedrive/primitives';
import { dialogManager, useDialog, Dialogs } from '@spacedrive/primitives';

// Forms
import { Form, InputField, SelectField, CheckboxField } from '@spacedrive/forms';

// AI
import { ToolCall, Markdown, ChatComposer, ModelSelector, TaskRow } from '@spacedrive/ai';
import type { ToolCallPair, Task, TaskStatus, ModelOption } from '@spacedrive/ai';

// Explorer
import { TagPill, RenameInput, FileThumb, GridItem } from '@spacedrive/explorer';

// Icons
import { getIcon, getBeardedIcon } from '@spacedrive/icons/util';
```

## Contributing Conventions

- Bun only. TypeScript strict, no `any`.
- `forwardRef` for ref forwarding, `"use client"` for client components
- Semantic Tailwind classes only (`bg-app`, `text-ink`). Never `var()`. Never hardcoded colors.
- PascalCase components, camelCase utilities. One component per file.
- Composable over configurable (Card.Header vs 15 props)
- CVA for variant styling
- Changesets required for user-facing changes
