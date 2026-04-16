## 1. Code Fixes

- [ ] 1.1 Remove no-op `.replace(/claude-/, "claude-")` from `formatModelName` in `interface/src/routes/AgentDetail.tsx:820`
- [ ] 1.2 Replace ReDoS-susceptible regex `/\/+$/` with safe alternative in `packages/api-client/src/client.ts:21`

## 2. Dismiss CodeQL False Positives

- [ ] 2.1 Dismiss hard-coded crypto value alerts #3, #4, #5, #6, #7, #8 via `gh api` with `false_positive` reason
- [ ] 2.2 Dismiss cleartext logging alerts #9, #10, #11, #12, #13, #14, #15, #16, #17, #18 via `gh api` with `false_positive` reason
- [ ] 2.3 Dismiss cleartext transmission alerts #19, #20, #21, #22, #23 via `gh api` with `false_positive` reason

## 3. Dismiss Dependabot False Positive

- [ ] 3.1 Dismiss Dependabot alert #17 (`glib`) via `gh api` with `inaccurate` reason

## 4. Verify

- [ ] 4.1 Confirm CodeQL open alert count is 2 or fewer (pending rescan of code fixes)
- [ ] 4.2 Confirm Dependabot open alert count is 5 (4 deferred Rust deps)
- [ ] 4.3 Commit code fixes and push to trigger CodeQL rescan
