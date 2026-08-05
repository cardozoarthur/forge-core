"""Deprecated Forge class aliases backed by the canonical Foundry SDK."""

from foundry_sdk.workflow import (
    FoundryClient,
    FoundryInvocation,
    FoundryNode,
    FoundryResult,
    FoundryWorkflow,
)

ForgeClient = FoundryClient
ForgeInvocation = FoundryInvocation
ForgeNode = FoundryNode
ForgeResult = FoundryResult
ForgeWorkflow = FoundryWorkflow

__all__ = [
    "ForgeClient",
    "ForgeInvocation",
    "ForgeNode",
    "ForgeResult",
    "ForgeWorkflow",
]
