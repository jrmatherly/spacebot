## ADDED Requirements

### Requirement: No identity string replacements in model name formatting

The `formatModelName` function in AgentDetail.tsx SHALL NOT contain `.replace()` calls that replace a string with itself. All replacements MUST produce a different output than their input.

#### Scenario: Identity replacement removed
- **WHEN** `formatModelName("claude-sonnet-4-20250514")` is called
- **THEN** the no-op `.replace(/claude-/, "claude-")` line SHALL NOT exist in the function body

#### Scenario: Date suffix stripping still works
- **WHEN** `formatModelName("anthropic/claude-sonnet-4-20250514")` is called
- **THEN** the result SHALL be `"claude-sonnet-4"` with date suffixes stripped and provider prefix removed

### Requirement: URL setter avoids ReDoS-susceptible regex on uncontrolled input

The `setServerUrl` function in api-client SHALL strip trailing slashes without using a regex pattern that exhibits polynomial time complexity on pathological input.

#### Scenario: Trailing slashes stripped
- **WHEN** `setServerUrl("http://localhost:19898///")` is called
- **THEN** `getServerUrl()` SHALL return `"http://localhost:19898"`

#### Scenario: No trailing slash unchanged
- **WHEN** `setServerUrl("http://localhost:19898")` is called
- **THEN** `getServerUrl()` SHALL return `"http://localhost:19898"`

### Requirement: False positive CodeQL alerts dismissed with documented reasons

All CodeQL alerts verified as false positives SHALL be dismissed via the GitHub API with `dismissed_reason: "false_positive"` and a `dismissed_comment` explaining why.

#### Scenario: Crypto buffer alerts dismissed
- **WHEN** CodeQL alerts #3-#8 (hard-coded cryptographic value) are reviewed
- **THEN** each SHALL be dismissed with reason explaining buffer initialization pattern

#### Scenario: Logging alerts dismissed
- **WHEN** CodeQL alerts #9-#18 (cleartext logging) are reviewed
- **THEN** each SHALL be dismissed with reason explaining test fixtures or non-secret logged data

#### Scenario: Transmission alerts dismissed
- **WHEN** CodeQL alerts #19-#23 (cleartext transmission) are reviewed
- **THEN** each SHALL be dismissed with reason explaining localhost-only communication
