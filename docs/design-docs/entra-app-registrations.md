# Entra App Registrations for Spacebot

> Source decision: research §11.2(2), §11.4, §12 E-4.

## Registrations

Two registrations in the single Entra tenant:

### 1. `spacebot-web-api` (confidential client)

- **Purpose:** Represents the Spacebot daemon as a protected Web API.
- **Type:** Web / confidential client.
- **Supported account types:** Single tenant.
- **Exposed scope:** `api.access` (delegated). Application ID URI: leave as `api://{client-id-guid}` default.
- **App roles:** `SpacebotAdmin`, `SpacebotUser`, `SpacebotService` (defined as `allowedMemberTypes: ["User", "Application"]` for service-principal assignment).
- **API permissions required:**
  - Microsoft Graph → `Group.Read.All` (application) if Phase 3 picks app-only; or `GroupMember.Read.All` (delegated) if OBO.
- **Manifest override:** `accessTokenAcceptedVersion: 2` — forces v2 tokens with `aud = <client-id-guid>` instead of the Application ID URI.
- **Client credential:** Certificate (preferred) or client secret stored under `ENTRA_GRAPH_CLIENT_SECRET` in Spacebot's secret store.

### 2. `spacebot-spa` (public client, SPA platform)

- **Purpose:** Browser-based web UI bundled in the daemon binary.
- **Type:** Single-page application (SPA) platform configuration.
- **Supported account types:** Single tenant.
- **Redirect URIs (SPA type):**
  - Production: `https://{deployment-host}/` (exact origin, trailing slash included).
  - Local hosted dev: `http://localhost:19898/` (for bundled-server development).
  - Vite dev server: `http://localhost:19840/` (for `bun run dev` in `interface/`).
- **Redirect URIs (Mobile/Desktop type, added later for Tauri in Phase 8):**
  - `http://127.0.0.1` — IP literal, NOT `localhost`. Must be added via manifest `replyUrlsWithType` attribute; the Azure portal UI blocks direct entry of IP literals (§12 E-6).
- **API permissions:** Delegated `api.access` scope from `spacebot-web-api`.
- **Admin consent:** granted in tenant before rollout (users never see consent prompt for delegated Graph).

## Why two, not one

An app registration can technically expose SPA + Web + Mobile/Desktop platform configurations simultaneously. We split because:

1. The SPA must not hold a client secret (public-client constraint); the Web API must.
2. App roles defined on the Web API registration flow through token issuance to the SPA as `roles` claim without further wiring.
3. Separating the registrations limits blast radius: a compromise of the SPA flow doesn't grant the Graph `Group.Read.All` permission that lives on the Web API registration.

## Outstanding ops decisions

- Which team in IT provisions and rotates the certificate for `spacebot-web-api`?
- Conditional Access policy name(s) that must apply: MFA required for `SpacebotAdmin` role, compliant device required for device-code completion (see research §12 S-C3).
- Pre-prod tenant separate from production per §12 CC8.1 — name it.
