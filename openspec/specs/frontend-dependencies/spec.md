# Frontend Dependencies

## Purpose
Version currency for frontend packages across the interface and docs sites. Covers TypeScript, icon libraries, and build tool compatibility.

## Requirements

### Requirement: TypeScript upgraded to 6.0
The project SHALL upgrade TypeScript from 5.9 to 6.0 in both `interface/` and `docs/`. The tsconfig files SHALL be updated to explicitly set `"types": ["vite/client"]` (interface) and any other necessary defaults that changed in TS6.

#### Scenario: Interface typecheck passes
- GIVEN TypeScript is bumped to 6.0 in `interface/`
- WHEN `bunx tsc --noEmit` is run in `interface/`
- THEN the typecheck passes with zero errors

#### Scenario: Docs build passes
- GIVEN TypeScript is bumped to 6.0 in `docs/`
- WHEN `bun run build` is run in `docs/`
- THEN the Next.js static build completes successfully

### Requirement: Lucide-react upgraded to 1.8 in docs
The project SHALL upgrade lucide-react from 0.563 to 1.8 in the docs site. Any usage of removed brand icons SHALL be replaced with alternatives.

#### Scenario: No removed brand icons in use
- GIVEN the lucide-react upgrade is applied
- WHEN a grep for removed brand icon names is run across `docs/`
- THEN zero matches are found

#### Scenario: Docs build succeeds after upgrade
- GIVEN the lucide-react upgrade is applied
- WHEN `bun run build` is run in `docs/`
- THEN the build completes successfully

### Requirement: Upgrade @lobehub/icons to 5.4.0
The system SHALL use `@lobehub/icons` version 5.4.0 in `interface/package.json`. All 14 existing `es/` subpath icon imports SHALL continue to resolve without code changes.

#### Scenario: Version bump with no code changes
- GIVEN `@lobehub/icons` is bumped from `^4.12.0` to `^5.4.0` in `interface/package.json`
- WHEN `bun install` and `bun run build` are run
- THEN the production build succeeds with no import resolution errors

#### Scenario: All provider icons resolve
- GIVEN the frontend builds with `@lobehub/icons@5.4.0`
- WHEN all 14 imports in `interface/src/lib/providerIcons.tsx` are resolved
- THEN Anthropic, OpenAI, OpenRouter, Groq, Mistral, DeepSeek, Fireworks, Together, XAI, ZAI, Minimax, Kimi, Google, and GithubCopilot all resolve successfully

### Requirement: No unnecessary peer dependencies added
The system SHALL NOT add `antd` or `@lobehub/ui` as dependencies. The unmet peer dep warnings are expected and harmless for the `es/` subpath usage pattern.

#### Scenario: antd remains absent
- GIVEN the icon upgrade is complete
- WHEN `interface/package.json` is inspected
- THEN it does not contain `antd` or `@lobehub/ui` in dependencies or devDependencies
