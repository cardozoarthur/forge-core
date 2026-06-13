# Forge File Workflow Contract

Data: 2026-06-13

File creation, normalization and export are workflows in Forge.

## Why this matters

Creating a file is not just writing bytes:

- collect source data
- organize the data
- transform it into the target schema
- validate the output shape
- attach provenance and session context
- persist the artifact in the correct place

That is a workflow, because it has inputs, rules, intermediate state and a durable output.

## Forge interpretation

The Forge runtime should treat file production as a workflow or subworkflow whenever the task is non-trivial, repetitive, structured or session-specific.

Examples:

- generate a report markdown file
- create a structured config file
- export a benchmark artifact
- assemble a long document from multiple sources
- write a session-scoped file with canonical naming and schema

## Expected stages

1. inspect source inputs
2. normalize and sort the data
3. choose the target schema or template
4. render the file contents
5. validate the structure
6. write the file
7. attach the artifact to the workflow or session

## Placement

This behavior belongs in Core workflow logic and artifact contracts. Addons may provide domain-specific exporters, but the "make a file" action itself should be routed as workflow when it is more than a trivial single-line write.

