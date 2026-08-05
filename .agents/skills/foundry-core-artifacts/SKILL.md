---
name: foundry-core-artifacts
description: Foundry Core workflow artifacts, tags, documents, reports, listing, and bounded fetching.
license: MIT
compatibility: codex, opencode, agy, claude
---

## Artifact Contract

Artifacts are workflow state. Attach documents, proposals, contracts, reports, campaigns, emails and generated files through Foundry so lineage and tags stay auditable.

Use:

```bash
foundry workflow attach-artifact --workflow <workflow-id> --path <path> --kind report --tag <tag> --origin codex --output json
foundry mcp call foundry.workflow.attach_artifact --input '{"workflow_id":"<workflow-id>","path":"<path>","kind":"report","tags":["crm","proposal"],"origin":"codex"}' --output json
foundry artifacts --workflow <workflow-id> --output json
foundry mcp call foundry.artifact.fetch --input '{"workflow_id":"<workflow-id>","path":"<artifact-path>"}' --output json
```

Use tags for artifact kind, workflow stage, account/customer, domain and search intent. Do not create parallel artifact registries outside Foundry.

## Attaching Artifacts

When you attach an artifact, Foundry copies it into workflow-scoped artifact storage, links it to the workflow, and computes its SHA-256 hash.

### Attaching via CLI
To attach a file:
```bash
foundry workflow attach-artifact --workflow <workflow-id> --path "/absolute/path/to/report.pdf" --kind report --tag "billing" --tag "v1.0" --origin "agent-qa" --output json
```

### Recording Source Lineage
The current CLI does not expose a dedicated `derived-from` field. Attach the source artifact separately and use a stable lineage tag until structured artifact relationships are available:
```bash
foundry workflow attach-artifact --workflow <workflow-id> --path "/path/to/draft.txt" --kind source --tag "lineage:draft" --origin codex --output json
foundry workflow attach-artifact --workflow <workflow-id> --path "/path/to/final.pdf" --kind report --tag "derived-from:draft.txt" --origin codex --output json
```

## Fetching Artifacts

You can retrieve, view, or download artifacts associated with any active or archived workflow.

### Listing Artifacts
To list all artifacts for a given workflow:
```bash
foundry artifacts --workflow <workflow-id> --output json
```

### Fetching a Specific Artifact
To retrieve artifact metadata, and optionally bounded UTF-8 content, copy the exact relative `path` returned by `foundry artifacts` into the MCP request:
```bash
foundry mcp call foundry.artifact.fetch --input '{"workflow_id":"<workflow-id>","path":"<artifact-path>","max_bytes":65536}' --output json
```

### Preserving Historical Versions
Artifact fetch has no revision selector. Preserve a historical version by attaching it under a distinct filename or version tag, then fetch the exact path and verify the returned SHA-256.

## Tags and Metadata Taxonomy

Tags allow fast query capability across workflows and projects. We enforce a structured tag taxonomy:

- **Type Tags**: `type:report`, `type:proposal`, `type:source-code`, `type:test-results`.
- **Domain/Context Tags**: `domain:billing`, `domain:crm`, `domain:compliance`.
- **Stage Tags**: `stage:draft`, `stage:final`, `stage:reviewed`.
- **Search Intent Tags**: `intent:audit`, `intent:delivery`.

Ensure every attached artifact has at least one **type** tag and one **domain** tag.
