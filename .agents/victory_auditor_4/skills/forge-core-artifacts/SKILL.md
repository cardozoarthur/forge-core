---
name: forge-core-artifacts
description: Forge Core workflow artifacts, tags, documents, reports, lineage, versioning, and fetching.
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

## Attaching Artifacts

When you attach an artifact, Forge stores it in the SQLite database or configured file storage, links it to the current task execution node, and computes its SHA-256 hash.

### Attaching via CLI
To attach a file:
```bash
forge workflow attach-artifact --workflow <workflow-id> --artifact "/absolute/path/to/report.pdf" --kind report --tag "billing" --tag "v1.0" --origin "agent-qa" --output json
```

### Lineage Auto-Extraction
Forge automatically extracts lineage from attached artifacts. If the file is derived from another artifact, record the relationship:
```bash
forge workflow attach-artifact --workflow <workflow-id> --artifact "/path/to/final.pdf" --kind report --derived-from "/path/to/draft.txt" --output json
```

## Fetching Artifacts

You can retrieve, view, or download artifacts associated with any active or archived workflow.

### Listing Artifacts
To list all artifacts for a given workflow:
```bash
forge artifacts --workflow <workflow-id> --output json
```

### Fetching a Specific Artifact
To retrieve the metadata and local content path of an artifact:
```bash
forge artifact fetch --workflow <workflow-id> --path "report.pdf" --output json
```

### Tracking Historical Versions
If an artifact path is updated, Forge revisions it. To fetch a specific historic version, supply the version revision ID:
```bash
forge artifact fetch --workflow <workflow-id> --path "report.pdf" --revision 3 --output json
```

## Tags and Metadata Taxonomy

Tags allow fast query capability across workflows and projects. We enforce a structured tag taxonomy:

- **Type Tags**: `type:report`, `type:proposal`, `type:source-code`, `type:test-results`.
- **Domain/Context Tags**: `domain:billing`, `domain:crm`, `domain:compliance`.
- **Stage Tags**: `stage:draft`, `stage:final`, `stage:reviewed`.
- **Search Intent Tags**: `intent:audit`, `intent:delivery`.

Ensure every attached artifact has at least one **type** tag and one **domain** tag.
