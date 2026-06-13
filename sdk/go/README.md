# Forge Go SDK

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
