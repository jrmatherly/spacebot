# @spacedrive/explorer

File-surface primitives for Spacedrive and Spacebot applications.

The explorer package ships only the self-contained building blocks. Larger, stateful views (FileGrid, FileList, PathBar, Inspector, QuickPreview, DragOverlay, KindIcon, FileRow, etc.) live in each consuming app because they are tightly coupled to the app's data layer, virtualization strategy, drag-drop context, and platform behaviors. See [`../../docs/COMPONENT-AUDIT.md`](../../docs/COMPONENT-AUDIT.md) for rationale.

## Installation

```bash
bun add @spacedrive/explorer @spacedrive/primitives
# or
npm install @spacedrive/explorer @spacedrive/primitives
```

Peer dependencies:
- `react` ^18.0.0 || ^19.0.0
- `react-dom` ^18.0.0 || ^19.0.0

## Usage

```tsx
import { FileThumb, GridItem, RenameInput, TagPill } from '@spacedrive/explorer';
```

## Components

### FileThumb

File thumbnail renderer. Accepts a thumbnail URL or a kind identifier and renders an appropriate visual.

### GridItem

Grid-cell shell used by consuming apps to wrap any content (thumbnail + label + selection state) in a consistent layout. Does not own selection or DnD — the app passes handlers via props.

### RenameInput

Inline rename field. Extension-aware (preserves file extension through edit), async save, blur cancellation. Generic over the item shape being renamed.

```tsx
<RenameInput
  initialValue="document.pdf"
  onRename={async (next) => save(next)}
  onCancel={() => setEditing(false)}
/>
```

### TagPill

Colored tag pill with optional remove button. Accepts a `color` + `children` API.

```tsx
<TagPill color="#ef4444">Important</TagPill>

<TagPill color="#3b82f6" onRemove={() => removeTag(id)}>
  Project X
</TagPill>
```

## What lives in consuming apps

The following Explorer pieces intentionally stay in `@sd/interface` or each app's codebase, not here:

- `FileGrid` / `FileList` / `FileRow` — TanStack Virtual + Table, dnd-kit, context menus
- `PathBar` — SdPath, device system, routing, animations
- `Inspector` / `InspectorPanel` — polymorphic variants tied to data types
- `KindIcon` — Rust-generated asset system
- `FileThumb` (full variant) — sidecar system, caching, video scrubber
- `QuickPreview` — standalone Tauri window
- `DragOverlay` — integrated with DndProvider

See [`../../docs/COMPONENT-AUDIT.md`](../../docs/COMPONENT-AUDIT.md) for the full breakdown.

## Design Principles

1. **Data via props** — Components don't fetch data
2. **Platform-agnostic** — React DOM only, no platform APIs
3. **Callback-driven** — Events via props, not internal state
4. **Small surface** — We export the pieces apps can genuinely share, and resist the urge to re-platform the whole explorer here

## Browser Support

- Chrome/Edge 88+
- Firefox 78+
- Safari 14+

## License

MIT © Spacedrive
