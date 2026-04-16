# SpaceUI Repository Summary

## Overview

A monorepo housing the Spacedrive ecosystem design system: 6 packages covering design tokens, base primitives, form wrappers, file-type icons, AI-agent surfaces, and explorer primitives.

## Package Structure

### @spacedrive/tokens (0.2.3)
- CSS-first design tokens for Tailwind v4 (`@theme` block)
- Semantic color system, spacing, radii, fonts
- Themes: dark (default), light, midnight, noir, slate, nord, mocha
- Key exports: `@spacedrive/tokens/theme`, `@spacedrive/tokens/css`, `@spacedrive/tokens/raw-colors`

### @spacedrive/primitives (0.2.3)
- 41 base UI components built on Radix + Headless UI
- Key exports: Button, Input, Dialog, Dropdown, Popover, Select, Tabs, Tooltip, Toast, Card, Badge, and more

### @spacedrive/forms (0.2.3)
- 7 form field wrappers built on react-hook-form
- Key exports: Form, InputField, TextAreaField, SelectField, CheckboxField, RadioGroupField, SwitchField

### @spacedrive/icons (0.2.3)
- Spacedrive file-type icons, extension badges, and icon resolution utilities
- Ships raw SVG assets (no React components) plus a `getIcon` resolver
- Key exports: `@spacedrive/icons/icons`, `@spacedrive/icons/svgs/*`, `@spacedrive/icons/util`

### @spacedrive/ai (0.2.3)
- 12 AI/agent interaction components
- Key exports: ToolCall, Markdown, MessageBubble, ChatComposer, ModelSelector, InlineWorkerCard, InlineBranchCard, TaskList, TaskRow, TaskDetail, TaskCreateForm, TaskStatusIcon, TaskPriorityIcon

### @spacedrive/explorer (0.2.3)
- 3 file-surface primitives (FileThumb, GridItem, RenameInput, TagPill)
- Larger explorer views (FileGrid/FileList/PathBar/Inspector/QuickPreview) live in each consuming app

## Development Tooling

### Build System
- **Package Manager**: Bun workspaces (bun 1.1.0+)
- **Build Tool**: tsup for JS/TS packages; tokens is CSS-only
- **Orchestration**: Turbo
- **TypeScript**: Strict mode, TypeScript 6

### Development Environment
- **Showcase App**: Vite + React demo app
- **Storybook**: 10.3.5 (component documentation, port 6006)
- **Scripts**: link-packages.sh, unlink-packages.sh, build-watch.sh

### Styling
- Tailwind v4 (CSS-first configuration via `@theme`)
- Consumers `@source` spaceui package source paths
- `@plugin` directive for Tailwind plugins (consumers decide)

### Release
- **Changesets**: versioning with linked packages (primitives/forms/ai/explorer/icons release together)
- **Publishing**: manual `bunx changeset publish` to npm under `@spacedrive` scope
- **Pre-release**: `bunx changeset pre enter <tag>` for alpha/beta trains

## Documentation

### Repo-level
- `README.md` - overview & quick start
- `CONTRIBUTING.md` - contributor guide
- `INTEGRATION.md` - consuming from an external project
- `LICENSE` - MIT

### docs/
- `SHARED-UI-STRATEGY.md` - migration plan & architecture
- `TAILWIND-V4-MIGRATION.md` - v3→v4 migration spec
- `COMPONENT-AUDIT.md` - fidelity audit against real Spacedrive UI
- `REPO_SUMMARY.md` - this file

### Package READMEs
- `packages/tokens/README.md`
- `packages/primitives/README.md`
- `packages/forms/README.md`
- `packages/ai/README.md`
- `packages/explorer/README.md`
- (icons has no README — see `packages/icons/package.json` description)

## Key Configuration Files
- `turbo.json`
- `tsconfig.base.json`
- `.changeset/config.json`
- `.storybook/main.ts`, `.storybook/preview.ts`
- `.github/workflows/ci.yml` (bot-authored PRs skipped via claude-review config)

## Quick Commands

```bash
# Development
bun install          # Install dependencies
bun run build        # Build all packages (turbo)
bun run dev          # Watch mode
bun run typecheck    # Type check all packages
bun run showcase     # Run demo app
bun run storybook    # Start Storybook

# Local linking
bun run link         # Link packages into global registry
bun run unlink       # Unlink

# Publishing
bun run changeset           # Create changeset
bun run version-packages    # Bump versions
bun run publish             # Build + publish to npm
```

---

Built with ❤️ for the Spacedrive ecosystem
