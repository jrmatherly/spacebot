## ADDED Requirements

### Requirement: Upgrade @lobehub/icons to 5.4.0
The system SHALL use `@lobehub/icons` version 5.4.0 in `interface/package.json`. All 14 existing `es/` subpath icon imports SHALL continue to resolve without code changes.

#### Scenario: Version bump with no code changes
- **WHEN** `@lobehub/icons` is bumped from `^4.12.0` to `^5.4.0` in `interface/package.json`
- **THEN** `bun install` succeeds and `bun run build` produces a successful production build with no import resolution errors

#### Scenario: All provider icons resolve
- **WHEN** the frontend builds with `@lobehub/icons@5.4.0`
- **THEN** all 14 imports in `interface/src/lib/providerIcons.tsx` (Anthropic, OpenAI, OpenRouter, Groq, Mistral, DeepSeek, Fireworks, Together, XAI, ZAI, Minimax, Kimi, Google, GithubCopilot) resolve successfully

### Requirement: No unnecessary peer dependencies added
The system SHALL NOT add `antd` or `@lobehub/ui` as dependencies. The unmet peer dep warnings are expected and harmless for our `es/` subpath usage pattern.

#### Scenario: antd remains absent
- **WHEN** the upgrade is complete
- **THEN** `interface/package.json` does not contain `antd` or `@lobehub/ui` in dependencies or devDependencies
