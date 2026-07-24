---
name: forge-core-artifacts
description: Forge Core workflow artifacts, tags, documents, reports and lineage.
license: MIT
compatibility: codex, opencode, gemini, claude
---

## Artifact Contract

Artifacts are workflow state. Attach documents, proposals, contracts, reports, campaigns, emails and generated files through Forge so lineage and tags stay auditable.

Use:

```bash
forge workflow attach-artifact --workflow <workflow-id> --artifact <path> --kind report --tag <tag> --origin codex --output json
forge mcp call forge.workflow.attach_artifact --input '{"workflow_id":"<workflow-id>","path":"<path>","kind":"report","tags":["crm","proposal"],"origin":"codex"}' --output json
forge artifacts --workflow <workflow-id> --output json
forge mcp call forge.artifact.fetch --input '{"workflow_id":"<workflow-id>","path":"<artifact-path>"}' --output json
```

Use tags for artifact kind, workflow stage, account/customer, domain and search intent. Do not create parallel artifact registries outside Forge.
