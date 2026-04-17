# SpaceUI (vendored)

A shared design system for Spacedrive and Spacebot applications.

> **Heads up:** You're reading this README from inside the Spacebot repository. SpaceUI originated as a standalone monorepo but lives here in-tree as an independent Bun workspace (own `bun.lock`, own Turborepo config, own changeset flow). The Spacebot web UI (`interface/`) consumes these packages via bun's workspace protocol: `interface/package.json` declares `"workspaces": ["../spaceui/packages/*"]` and pins each dep to `"workspace:*"`. There is no separate `spaceui` repo to clone.

## Overview

SpaceUI houses all shared UI components, design tokens, and styling utilities for the Spacebot and Spacedrive interfaces. Cloning the Spacebot repo gives you the full source.

## Package Structure

```
spaceui/
├── packages/
│   ├── tokens/         # @spacedrive/tokens - CSS-first design tokens (Tailwind v4)
│   ├── primitives/     # @spacedrive/primitives - Base UI components (42)
│   ├── forms/          # @spacedrive/forms - react-hook-form wrappers (7)
│   ├── icons/          # @spacedrive/icons - file-type icons + extension badges (SVG)
│   ├── ai/             # @spacedrive/ai - AI agent components (13)
│   └── explorer/       # @spacedrive/explorer - File management (4)
├── examples/
│   └── showcase/       # Interactive demo app
├── .storybook/         # Component documentation
└── scripts/            # Development utilities
```

## Quick Start

### Installation

Clone the Spacebot repo (which contains this directory), then work from `spaceui/`.

```bash
git clone https://github.com/jrmatherly/spacebot
cd spacebot/spaceui

# Install dependencies
bun install

# Build all packages
bun run build
```

### Development

```bash
# Start development mode with file watching
bun run dev

# Run the showcase app
bun run showcase

# Start Storybook
bun run storybook

# Build specific package
bun run watch primitives

# Type check
bun run typecheck

# Clean build artifacts
bun run clean
```

### Consuming from `interface/`

The Spacebot web UI resolves `@spacedrive/*` imports via bun's workspace protocol; no manual linking step is required. From the repo root:

```bash
cd interface
bun install          # creates symlinks from interface/node_modules/@spacedrive/* into ../spaceui/packages/*
bun run dev          # Vite dev server; Rolldown resolves source .tsx through the symlinks
```

Before running `bunx tsc --noEmit` from `interface/`, run `just spaceui-build` at the repo root so each package's `dist/index.d.ts` is up to date. Vite dev/build does not need the prebuild.

## Using SpaceUI

### In a Spacedrive/Spacebot Application

```typescript
// Import raw semantic token values when needed
import colors from '@spacedrive/tokens/raw-colors';

// Import primitives
import { Button, Card, Dialog } from '@spacedrive/primitives';

// Import form fields
import { InputField, SelectField } from '@spacedrive/forms';

// Import file-type icons and resolution helpers
import { getIcon } from '@spacedrive/icons/util';

// Import AI components
import { ToolCall, ChatComposer, Markdown } from '@spacedrive/ai';

// Import explorer components
import { FileThumb, RenameInput, TagPill } from '@spacedrive/explorer';
```

### Tailwind Configuration

```css
/* Tailwind v4 CSS entrypoint */
@import '@spacedrive/tokens/theme';
```

### CSS Setup

```css
/* In your app's base CSS */
@import '@spacedrive/tokens/css';
```

## Packages

### @spacedrive/tokens

Design tokens package with CSS entrypoints and raw semantic color values.

**Exports:**
- `@spacedrive/tokens/css` - Base token layer
- `@spacedrive/tokens/theme` - Theme variable layer
- `@spacedrive/tokens/raw-colors` - Programmatic semantic color map
- CSS files for dark/light themes

[Read more →](./packages/tokens/README.md)

### @spacedrive/primitives

Base UI components built on Radix UI primitives.

**Components:**
- **Interactive:** Button, Input, Checkbox, Switch, Slider, RadioGroup
- **Overlay:** Dialog, Popover, Tooltip, DropdownMenu, ContextMenu
- **Navigation:** Tabs, Select, Dropdown
- **Display:** Badge, Card, Banner, Toast, Loader, Divider, Typography, Shortcut
- **Form:** NumberStepper, FilterButton, ToggleGroup, SearchBar
- **Progress:** ProgressBar, CircularProgress
- **Layout:** Resizable panels, Collapsible, TopBarButton

[Read more →](./packages/primitives/README.md)

### @spacedrive/forms

Form field wrappers built on react-hook-form.

**Components:**
- Form, FormField, FormItem, FormLabel, FormControl, FormDescription, FormMessage
- InputField, TextAreaField, SelectField, CheckboxField, RadioGroupField, SwitchField

[Read more →](./packages/forms/README.md)

### @spacedrive/icons

File-type icons, extension badges, and icon resolution utilities. Ships raw SVG assets plus a `getIcon` resolver keyed on kind/extension. No React components — consumers render the SVGs themselves.

**Exports:**
- `@spacedrive/icons/icons` - React icon index
- `@spacedrive/icons/svgs/*` - raw SVG assets
- `@spacedrive/icons/util` - `getIcon(name, kind?)` resolver

### @spacedrive/ai

AI agent interaction components.

**Components:**
- `ToolCall` - Tool invocation display
- `Markdown` - Agent response renderer
- `InlineWorkerCard`, `InlineBranchCard` - Worker/branch task cards with transcript
- `ChatComposer` - Message input with model selection
- `ModelSelector` - LLM model picker
- `MessageBubble` - Agent/user message shell
- `TaskList`, `TaskRow`, `TaskDetail`, `TaskCreateForm` - Task surface
- `TaskStatusIcon`, `TaskPriorityIcon` - Task metadata glyphs

[Read more →](./packages/ai/README.md)

### @spacedrive/explorer

File-surface primitives. Larger explorer views (FileGrid/FileList/PathBar/Inspector/QuickPreview) live in each consuming app — the UI library keeps only the self-contained pieces.

**Components:**
- `FileThumb` - File thumbnail renderer
- `GridItem` - Grid cell shell
- `RenameInput` - Inline rename field with extension awareness
- `TagPill` - Colored tag pill with optional remove button

[Read more →](./packages/explorer/README.md)

## Development Workflow

### Running the Showcase App

The showcase app demonstrates all components:

```bash
bun run showcase
# Opens at http://localhost:19850
```

### Storybook

Component documentation with interactive stories:

```bash
cd .storybook
bun install
bun run dev
# Opens at http://localhost:6006
```

### Creating a Changeset

We use [Changesets](https://github.com/changesets/changesets) for versioning. Changesets drive `CHANGELOG.md`. Never hand-edit those files.

```bash
# Create a changeset
bun run changeset

# Select packages and describe changes (creates .changeset/*.md)

# Version packages (consumes changesets, bumps package.json versions, rewrites CHANGELOGs)
bun run version-packages
```

Packages are consumed internally via the bun workspace protocol. They are not published to npm. External publishing would be a separate decision if the project direction changes.

## Migration Guide

See [docs/SHARED-UI-STRATEGY.md](./docs/SHARED-UI-STRATEGY.md) for the complete migration plan from existing Spacedrive and Spacebot UI codebases, and [docs/TAILWIND-V4-MIGRATION.md](./docs/TAILWIND-V4-MIGRATION.md) for the Tailwind v3→v4 migration spec.

Quick start for migration:

1. **Phase 1** - Move `ToolCall.tsx` and `Markdown.tsx` to stop duplication
2. **Phase 2** - Migrate primitives (Button, Input, etc.)
3. **Phase 3** - Extract AI components
4. **Phase 4** - Build new shared components
5. **Phase 5** - Extract explorer components
6. **Phase 6** - Cleanup old code

## Contributing

Contributions land via the Spacebot repository's PR workflow. See the root `CONTRIBUTING.md` for the overall flow and [`./CONTRIBUTING.md`](./CONTRIBUTING.md) for SpaceUI-specific conventions.

Quick contributing workflow:

1. Create a feature branch: `git checkout -b feature/my-feature`
2. Make your changes in the appropriate package
3. Add a changeset: `bun run changeset`
4. Run `bun run build` to ensure everything compiles
5. Run `bun run typecheck` to verify types
6. Commit your changes with a clear message
7. Push and create a pull request

## Scripts

| Script | Description |
|--------|-------------|
| `bun run build` | Build all packages |
| `bun run dev` | Watch mode for all packages |
| `bun run watch [package]` | Watch specific package |
| `bun run typecheck` | Type check all packages |
| `bun run clean` | Clean build artifacts |
| `bun run showcase` | Run demo app |
| `bun run changeset` | Create a changeset |
| `bun run version-packages` | Bump versions (consumes changesets, rewrites CHANGELOGs) |

## Resources

- [Design Strategy](./docs/SHARED-UI-STRATEGY.md) - Migration plan & architecture
- [Tailwind v4 Migration](./docs/TAILWIND-V4-MIGRATION.md) - v3→v4 migration spec
- [Component Audit](./docs/COMPONENT-AUDIT.md) - Fidelity check vs the Spacedrive components it was extracted from
- [Repository Summary](./docs/REPO_SUMMARY.md) - Monorepo stats & tooling
- [Contributing Guide](./CONTRIBUTING.md) - SpaceUI-specific development setup & guidelines
- [Integration Guide](./INTEGRATION.md) - Reference for consuming SpaceUI from an external project (not needed for this repo; `interface/` uses the workspace protocol)
- [Package READMEs](./packages/) - Individual package documentation
- [Radix UI](https://www.radix-ui.com/) - Primitives we build on
- [Tailwind CSS](https://tailwindcss.com/) - Styling system

## License

MIT. Origin: Spacedrive design system, vendored and maintained in the Spacebot repository.
