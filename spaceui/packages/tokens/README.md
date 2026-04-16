# @spacedrive/tokens

Design tokens for SpaceUI. CSS-first for Tailwind v4, with optional raw color exports for programmatic consumers.

## Installation

```bash
bun add @spacedrive/tokens
# or
npm install @spacedrive/tokens
```

## Usage

### Theme Entry Layer (Tailwind v4)

```css
@import "tailwindcss";

/* @theme block — generates all bg-*, text-*, border-*, ring-* utilities */
@import "@spacedrive/tokens/theme";

/* Base layer + default (dark) theme variables */
@import "@spacedrive/tokens/css";

/* Optional additional themes (opt in as needed) */
@import "@spacedrive/tokens/css/themes/light";
@import "@spacedrive/tokens/css/themes/midnight";
@import "@spacedrive/tokens/css/themes/noir";
@import "@spacedrive/tokens/css/themes/slate";
@import "@spacedrive/tokens/css/themes/nord";
@import "@spacedrive/tokens/css/themes/mocha";

@custom-variant dark (&:where(.dark, .dark *));
```

### Programmatic Access

```typescript
import colors from '@spacedrive/tokens/raw-colors';

// Access color values (returned as complete CSS color strings)
console.log(colors.accent.DEFAULT); // "hsl(208, 100%, 57%)"
console.log(colors.ink.dull);       // "hsl(235, 10%, 70%)"
```

## Color System

### Semantic Colors

All colors use semantic names rather than literal colors:

- **accent** - Primary brand color
- **ink** - Text colors (foreground)
- **app** - App backgrounds and surfaces
- **sidebar** - Sidebar-specific colors
- **menu** - Dropdown/menu colors
- **status** - Success, warning, error, info states

### Color Variants

Each color has variants:
- `DEFAULT` - Base color
- `faint` - Lighter variant
- `dull` - Muted variant
- `deep` - Darker variant

Example:
```css
accent           /* Primary accent */
accent-faint     /* Lighter */
accent-deep      /* Darker */

ink              /* Primary text */
ink-dull         /* Secondary text */
ink-faint        /* Tertiary text */
```

### CSS Custom Properties

Under Tailwind v4, the `@theme` block defines tokens as full CSS color values:

```css
@theme {
  --color-accent: hsl(208, 100%, 57%);
  --color-ink: hsl(235, 35%, 92%);
  --color-app: hsl(235, 15%, 13%);
  /* ... */
}
```

Opacity modifiers still work: `bg-accent/50`, `border-ink/20`, etc. — Tailwind v4 derives them automatically from `@theme` colors.

## Tailwind Classes

With `@import "@spacedrive/tokens/theme"` in your CSS, use semantic classes directly:

```tsx
<div className="bg-app text-ink">
  <button className="bg-accent text-white hover:bg-accent-deep">
    Click me
  </button>
  <p className="text-ink-dull">
    Secondary text
  </p>
</div>
```

### Opacity Modifiers

```tsx
<div className="bg-accent/10">    {/* 10% opacity */}
<div className="bg-sidebar/65">   {/* 65% opacity */}
```

## Themes

Themes override the base `--color-*` variables via CSS classes. The default theme is `dark` (loaded by `@spacedrive/tokens/css`). Opt in to any additional theme by importing it and toggling the class on `<html>` or any ancestor element.

```html
<html class="midnight-theme">
  <!-- all --color-* vars overridden to midnight values -->
</html>
```

Available themes: `dark` (default), `light`, `midnight`, `noir`, `slate`, `nord`, `mocha`.

## Consumer Pattern Summary

```css
@import "tailwindcss";
@import "@spacedrive/tokens/theme";         /* @theme block — generates utilities */
@import "@spacedrive/tokens/css";           /* base + default theme */
@import "@spacedrive/tokens/css/themes/midnight";  /* optional override */

@custom-variant dark (&:where(.dark, .dark *));

/* Tell Tailwind to scan your SpaceUI packages */
@source "../node_modules/@spacedrive/primitives/src";
@source "../node_modules/@spacedrive/ai/src";
@source "../node_modules/@spacedrive/forms/src";
@source "../node_modules/@spacedrive/explorer/src";
```

No `tailwind.config.js`. No JS preset. No build step for tokens.

## Design Principles

1. **CSS-first** — Tokens live in CSS. No JS build, no preset file.
2. **Semantic naming** — `ink`, `app-box`, `sidebar-selected`, not `gray-900`.
3. **Theme-agnostic** — Components use semantic classes; themes remap the variables.
4. **Native Tailwind v4 integration** — `@theme` drives utility generation; opacity modifiers work automatically.

## License

MIT © Spacedrive
