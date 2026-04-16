# @spacedrive/explorer

> Changelog entries between 0.1.1 and the current `package.json` version were not authored via changesets; see `git log packages/explorer` for the detail. The 0.1.0 "Packages Included" bullet list below predates the addition of `@spacedrive/icons` — spaceui now ships 6 packages. The explorer package itself now exports only self-contained primitives (FileThumb, GridItem, RenameInput, TagPill); see `packages/explorer/README.md` and `docs/COMPONENT-AUDIT.md`.

## 0.1.1

### Patch Changes

- workers, badge, card, ui updates
- Updated dependencies
  - @spacedrive/primitives@0.1.1

## 0.1.0

### Minor Changes

- Initial release of SpaceUI

  This is the first release of the SpaceUI design system for Spacedrive and Spacebot applications.

  ### Packages Included

  - **@spacedrive/tokens** - Design tokens and Tailwind preset with semantic color system
  - **@spacedrive/primitives** - 40+ base UI components built on Radix UI
  - **@spacedrive/forms** - Form field wrappers for react-hook-form
  - **@spacedrive/ai** - AI/agent interaction components (ToolCall, ChatComposer, etc.)
  - **@spacedrive/explorer** - File management components (FileGrid, Inspector, etc.)

  ### Features

  - Semantic color system (ink, app, accent, sidebar, menu)
  - Dark and light theme support
  - Accessible components via Radix UI
  - TypeScript strict mode throughout
  - CVA for component variants
  - Comprehensive showcase app

  ### Migration Status

  This release provides the foundation for migrating Spacedrive and Spacebot to a shared design system. See SHARED-UI-STRATEGY.md for the migration plan.

### Patch Changes

- Updated dependencies
  - @spacedrive/primitives@0.1.0
