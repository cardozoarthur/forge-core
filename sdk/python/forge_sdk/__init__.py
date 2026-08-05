"""Legacy Forge import aliases for the Foundry SDK 0.6.x migration window."""

from warnings import warn

from .workflow import (
    ForgeClient,
    ForgeInvocation,
    ForgeNode,
    ForgeResult,
    ForgeWorkflow,
)

warn(
    "forge_sdk is deprecated; import the canonical foundry_sdk package",
    DeprecationWarning,
    stacklevel=2,
)

__all__ = [
    "ForgeClient",
    "ForgeInvocation",
    "ForgeNode",
    "ForgeResult",
    "ForgeWorkflow",
]
