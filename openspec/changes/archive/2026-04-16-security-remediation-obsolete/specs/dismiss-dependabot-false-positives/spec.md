## ADDED Requirements

### Requirement: Dismiss inaccurate Dependabot alerts

Dependabot alerts for packages not present in the project's lockfiles SHALL be dismissed with `dismissed_reason: "inaccurate"` and a comment explaining the absence.

#### Scenario: glib alert dismissed
- **WHEN** Dependabot alert #17 for `glib` is reviewed
- **THEN** it SHALL be dismissed as inaccurate since `glib` has 0 matches in Cargo.lock
