# Contributing to SpaceUI

Thank you for your interest in contributing to SpaceUI! This guide will help you get started.

## Development Setup

### Prerequisites

- [Bun](https://bun.sh/) (v1.1.0 or later)
- Node.js 18+ (for compatibility)
- Git

### Getting Started

1. **Clone the repository**
   ```bash
   git clone https://github.com/spacedriveapp/spaceui.git
   cd spaceui
   ```

2. **Install dependencies**
   ```bash
   bun install
   ```

3. **Build all packages**
   ```bash
   bun run build
   ```

4. **Run the showcase app**
   ```bash
   cd examples/showcase
   bun run dev
   ```

## Project Structure

```
spaceui/
├── packages/
│   ├── tokens/         # Design tokens & Tailwind preset
│   ├── primitives/     # Base UI components
│   ├── forms/          # react-hook-form wrappers
│   ├── icons/          # File type icons and KindIcon component
│   ├── ai/             # AI agent components
│   └── explorer/       # File management components
├── examples/
│   └── showcase/       # Demo app
└── .changeset/         # Versioning
```

## Development Workflow

### Adding a New Component

1. **Choose the right package**
   - `@spacedrive/primitives` - Base building blocks (buttons, inputs)
   - `@spacedrive/forms` - Form field wrappers
   - `@spacedrive/ai` - Agent/AI components
   - `@spacedrive/explorer` - File management components

2. **Create the component file**
   ```typescript
   // packages/primitives/src/MyComponent.tsx
   import { clsx } from 'clsx';
   import { forwardRef } from 'react';
   
   interface MyComponentProps {
     // Props here
   }
   
   const MyComponent = forwardRef<HTMLDivElement, MyComponentProps>(
     ({ className, ...props }, ref) => {
       return (
         <div
           ref={ref}
           className={clsx('bg-app text-ink', className)}
           {...props}
         />
       );
     }
   );
   
   MyComponent.displayName = 'MyComponent';
   export { MyComponent };
   export type { MyComponentProps };
   ```

3. **Export from index.ts**
   ```typescript
   // packages/primitives/src/index.ts
   export { MyComponent } from './MyComponent';
   export type { MyComponentProps } from './MyComponent';
   ```

4. **Add to showcase app**
   Update `examples/showcase/src/App.tsx` to demonstrate the new component.

### Code Style

- **TypeScript**: Strict mode enabled (TypeScript 6). No `any` types.
- **Components**: Use `forwardRef` for ref forwarding.
- **Styling**: Tailwind v4. Use semantic classes (e.g., `bg-app`, `text-ink`).
- **Tailwind v4 syntax**: Use canonical shortcuts — `class!` (not `!class`), `data-disabled:` (not `data-[disabled]:`), `z-100` (not `z-[100]`), `*-(--X)` (not `*-[var(--X)]`).
- **Colors**: Never use `var()` directly. Use semantic classes only.
- **Naming**: PascalCase for components, camelCase for utilities.

### Testing Changes

1. **Type check**
   ```bash
   bun run typecheck
   ```

2. **Build**
   ```bash
   bun run build
   ```

3. **Test in showcase**
   ```bash
   cd examples/showcase && bun run dev
   ```

4. **Consume from interface/ (workspace protocol)**

   No linking needed. `interface/package.json` declares `"workspaces": ["../spaceui/packages/*"]`, so `bun install` inside `interface/` symlinks each `@spacedrive/*` package into `interface/node_modules`. Edit a file in `spaceui/packages/primitives/src/` and the Vite dev server (`cd interface && bun run dev`) picks it up via HMR.

   Before running `bunx tsc --noEmit` in `interface/`, rebuild spaceui so each package's `dist/index.d.ts` is current:

   ```bash
   cd spaceui && bun run build
   cd ../interface && bunx tsc --noEmit
   ```

## Making Changes

### Creating a Changeset

We use [Changesets](https://github.com/changesets/changesets) for versioning:

```bash
# Create a changeset
bun run changeset

# Select the packages you've modified
# Describe your changes
```

This creates a `.changeset/*.md` file describing your changes.

### Pull Request Process

1. Create a feature branch: `git checkout -b feature/my-feature`
2. Make your changes
3. Add a changeset: `bun run changeset`
4. Commit your changes
5. Push to your fork and create a PR
6. Ensure CI passes
7. Wait for review

### PR Requirements

- [ ] All packages build successfully
- [ ] Type checking passes
- [ ] Changeset added for user-facing changes
- [ ] Components follow design system conventions
- [ ] No hardcoded colors (use semantic classes)

## Component Design Principles

### Primitives

- **No business logic** - Just presentation
- **Accessible** - Wrap Radix primitives
- **Composable** - Prefer composition over configuration
- **Theme-aware** - Use semantic color classes

Example:
```tsx
// Good - composable
<Card>
  <CardHeader>
    <CardTitle>Title</CardTitle>
  </CardHeader>
  <CardContent>Content</CardContent>
</Card>

// Bad - too many props
<Card title="Title" content="Content" showHeader />
```

### AI Components

- **Data via props** - No internal fetching
- **Callbacks for events** - `onSend`, `onCancel`, etc.
- **Layout-agnostic** - Use flex/grid, let container constrain
- **Types co-located** - Export prop interfaces

### Explorer Components

- **Platform-agnostic** - React DOM only
- **Virtual-scroll ready** - Accept virtualizers
- **Thumbnail contract** - URL or kind identifier

## Common Issues

### "Module not found" errors

Make sure you've built the packages:
```bash
bun run build
```

### Changes not reflecting in showcase

The showcase uses workspace links, but you may need to restart the dev server:
```bash
# In examples/showcase
bun run dev
```

### Type errors in consuming apps

Ensure peer dependencies are installed:
- `react` & `react-dom`
- `tailwindcss`
- Package-specific peers (e.g., `react-hook-form` for forms)

## Resources

- [docs/SHARED-UI-STRATEGY.md](./docs/SHARED-UI-STRATEGY.md) - Migration plan
- [docs/TAILWIND-V4-MIGRATION.md](./docs/TAILWIND-V4-MIGRATION.md) - Tailwind v3→v4 migration spec
- [docs/COMPONENT-AUDIT.md](./docs/COMPONENT-AUDIT.md) - Fidelity audit vs real Spacedrive
- [Radix UI docs](https://www.radix-ui.com/) - Primitives we build on
- [Tailwind docs](https://tailwindcss.com/) - Styling system (v4)
- [CVA docs](https://cva.style/) - Component variants

## Questions?

- Open an issue for bugs or feature requests
- Start a discussion for questions
- Join our Discord for real-time chat

Thank you for contributing!
