# Foundry Go SDK

Status: scaffold

Purpose:
- backend orchestration
- service-side workflow execution
- infra integrations and workers

Contract:
- create a workflow handle
- run with context
- call subworkflows
- keep the same workflow identity for resume
- fan out service work and join other language workers when useful

The canonical module is `github.com/cardozoarthur/foundry-core/sdk/go` and its
package name is `foundry`. During the `0.6.x` migration window, legacy code may
temporarily import that module as `forge`, but the old repository path and <!-- foundry-brand-allow: legacy-compat -->
package name are deprecated:

```go
import forge "github.com/cardozoarthur/foundry-core/sdk/go" // foundry-brand-allow: legacy-compat
```
