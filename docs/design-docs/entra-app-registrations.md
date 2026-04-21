# Entra App Registrations for Spacebot

> **Status:** Spec. The Spacebot daemon validates v2 JWTs per this schema (Phase 1, PR #82, 2026-04-20). Tenant-side app-registration provisioning is operator-owned. Spacebot does not automate it. Phase 2 (PR #101, 2026-04-21) consumes the validated `AuthContext` to persist principal records. Phase 3 (2026-04-21) resolves Entra group memberships and user display photos via Microsoft Graph using delegated OBO. Phase 4+ will consume the roles defined below for handler-level authorization.

> Source decision: research §11.2(2), §11.4, §12 E-4.

## Registrations

Two registrations in the single Entra tenant:

### 1. `spacebot-web-api` (confidential client)

- **Purpose:** Represents the Spacebot daemon as a protected Web API.
- **Type:** Web / confidential client.
- **Supported account types:** Single tenant.
- **Exposed scope:** `api.access` (delegated). Application ID URI: leave as `api://{client-id-guid}` default.
- **App roles:** `SpacebotAdmin`, `SpacebotUser`, `SpacebotService` (defined as `allowedMemberTypes: ["User", "Application"]` for service-principal assignment).
- **API permissions required:** see "Graph API permissions" below.
- **Manifest override:** `accessTokenAcceptedVersion: 2` — forces v2 tokens with `aud = <client-id-guid>` instead of the Application ID URI.
- **Client credential:** Certificate (preferred) or client secret stored under `ENTRA_GRAPH_CLIENT_SECRET` in Spacebot's secret store.

### Graph API permissions (revised 2026-04-21)

Phase 3 resolves group memberships (`/me/getMemberObjects`) and user display photos (`/me/photo/$value`) through delegated **On-Behalf-Of (OBO)**. Both operations target the signed-in user only.

- **Delegated `User.Read`** on `spacebot-web-api` — primary path. OBO flow exchanges the user's access token for a Graph token scoped to the signed-in user. Covers `/me/getMemberObjects` (transitive group membership for overage resolution) and `/me/photo/$value` (A-19 photo fetch). Least-privileged delegated scope per Microsoft Learn for both endpoints. No tenant-wide read.
- **Application `User.Read.All`** on `spacebot-web-api` — reserved for display-name refresh on offline users (admin access reviews, SOC 2 evidence). Gated behind an admin-only endpoint. NOT used in Phase 3's per-request flow.
- **NOT requested:** `Group.Read.All`, `GroupMember.Read.All`. These grant tenant-wide or other-user enumeration. `User.Read` alone suffices because Spacebot only resolves the signed-in user's own memberships via `/me/getMemberObjects`, not other users'.

Why a single scope covers both operations: Microsoft Learn's permissions reference for `directoryObject: getMemberObjects` lists `User.Read` as the least-privileged delegated permission for the `/me/` path, and `profilePhoto: get` lists `User.Read` for `/me/photo/$value`. One OBO exchange per request serves both group sync and photo sync with minimal blast radius.

Certificate credential rotation: document the 90-day rollover window per Microsoft's guidance for `spacebot-web-api`.

Sources: `https://learn.microsoft.com/graph/api/directoryobject-getmemberobjects?view=graph-rest-1.0`, `https://learn.microsoft.com/graph/api/profilephoto-get?view=graph-rest-1.0`.

### 2. `spacebot-spa` (public client, SPA platform)

- **Purpose:** Browser-based web UI bundled in the daemon binary.
- **Type:** Single-page application (SPA) platform configuration.
- **Supported account types:** Single tenant.
- **Redirect URIs (SPA type):**
  - Production: `https://{deployment-host}/` (exact origin, trailing slash included).
  - Local hosted dev: `http://localhost:19898/` (for bundled-server development).
  - Vite dev server: `http://localhost:19840/` (for `bun run dev` in `interface/`).
- **Redirect URIs (Mobile/Desktop type, added later for Tauri in Phase 8):**
  - `http://127.0.0.1` — IP literal, NOT `localhost`. Must be added via manifest `replyUrlsWithType` attribute. The Azure portal UI blocks direct entry of IP literals (§12 E-6).
- **API permissions:** Delegated `api.access` scope from `spacebot-web-api`.
- **Admin consent:** granted in tenant before rollout (users never see consent prompt for delegated Graph).

## Why two, not one

An app registration can technically expose SPA + Web + Mobile/Desktop platform configurations simultaneously. We split because:

1. The SPA must not hold a client secret (public-client constraint). The Web API must.
2. App roles defined on the Web API registration flow through token issuance to the SPA as `roles` claim without further wiring.
3. Separating the registrations limits blast radius: a compromise of the SPA flow doesn't grant the delegated Graph `User.Read` (or reserved application `User.Read.All`) permission that lives on the Web API registration.

## Outstanding ops decisions

- Which team in IT provisions and rotates the certificate for `spacebot-web-api`?
- Conditional Access policy name(s) that must apply: MFA required for `SpacebotAdmin` role, compliant device required for device-code completion (see research §12 S-C3).
- Pre-prod tenant separate from production per §12 CC8.1. Name it.
