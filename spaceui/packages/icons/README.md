# @spacedrive/icons

File-type icons, extension badges, and icon-resolution utilities for SpaceUI. Ships SVG assets plus a `getIcon` resolver that maps file extensions to the right artwork. No React components. Consumers render the SVGs themselves.

## Installation

```bash
bun add @spacedrive/icons
# or
npm install @spacedrive/icons
```

## What's in the box

| Path | Contents |
|------|----------|
| `icons/` | React-compatible icon re-exports |
| `svgs/` | Raw SVG assets (file-type, extension badges) |
| `svgs/ext/icons.json` | Extension-to-icon mapping data |
| `util/` | `getIcon` resolver and helpers |

## Usage

### Resolve a file icon by extension

```typescript
import { getIcon } from '@spacedrive/icons/util';

const iconUrl = getIcon('document.pdf');
// returns the resolved SVG path for PDF files
```

### Import a raw SVG asset

```typescript
import FolderSVG from '@spacedrive/icons/svgs/Folder.svg';

<img src={FolderSVG} alt="Folder" />
```

### Use the extension-to-icon map directly

```typescript
import iconMap from '@spacedrive/icons/svgs/ext/icons.json';
```

## Regenerating the icon index

The icon index is generated from the SVG directory. If you add or rename SVG files under `svgs/`:

```bash
cd spaceui/packages/icons
bun run gen
```

This rebuilds `svgs/ext/icons.json` and the TypeScript re-exports under `icons/`.

## Peer dependencies

React 18 or 19 is required for the type re-exports in `icons/`. The package itself has no runtime dependencies.

## Versioning

This package versions independently from `@spacedrive/primitives`, `forms`, `ai`, and `explorer` (which are linked in `.changeset/config.json`). Version bumps to `@spacedrive/icons` do not cascade to the linked group.

## Related

- [`@spacedrive/tokens`](../tokens/README.md) — design tokens consumed alongside icons
- [`@spacedrive/primitives`](../primitives/README.md) — base UI components that often embed these icons
- SpaceUI root [INTEGRATION.md](../../INTEGRATION.md) — consuming packages from external apps
