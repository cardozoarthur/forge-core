from __future__ import annotations

import asyncio
from dataclasses import dataclass
from typing import Any, Awaitable, List, Optional


@dataclass
class ForgeInvocation:
    kind: str
    workflow_id: str
    node_id: Optional[str] = None
    resume_id: Optional[str] = None
    aggregator_id: Optional[str] = None
    input: Any = None
    branches: Optional[List[Any]] = None


@dataclass
class ForgeResult:
    base_url: str
    token_present: bool
    payload: ForgeInvocation


class ForgeNode:
    def __init__(self, client: "ForgeClient", workflow_id: str, node_id: str) -> None:
        self.client = client
        self.workflow_id = workflow_id
        self.node_id = node_id

    async def run(self, input: Any = None) -> ForgeResult:
        return await self.client._invoke(
            ForgeInvocation(
                kind="node",
                workflow_id=self.workflow_id,
                node_id=self.node_id,
                input=input,
            )
        )


class ForgeWorkflow:
    def __init__(self, client: "ForgeClient", workflow_id: str) -> None:
        self.client = client
        self.workflow_id = workflow_id

    def node(self, node_id: str) -> ForgeNode:
        return ForgeNode(self.client, self.workflow_id, node_id)

    async def run(self, input: Any = None) -> ForgeResult:
        return await self.client._invoke(
            ForgeInvocation(kind="workflow", workflow_id=self.workflow_id, input=input)
        )

    async def resume(self, resume_id: str) -> ForgeResult:
        return await self.client._invoke(
            ForgeInvocation(
                kind="resume",
                workflow_id=self.workflow_id,
                resume_id=resume_id,
            )
        )

    async def subworkflow(self, workflow_id: str) -> "ForgeWorkflow":
        return ForgeWorkflow(self.client, workflow_id)

    async def parallel(self, branches: List[Awaitable[Any]]) -> "_ForgeParallelJoin":
        results = await _gather(branches)
        return _ForgeParallelJoin(self.client, self.workflow_id, results)


class _ForgeParallelJoin:
    def __init__(self, client: "ForgeClient", workflow_id: str, branches: List[Any]) -> None:
        self.client = client
        self.workflow_id = workflow_id
        self.branches = branches

    async def join(self, aggregator_id: str) -> ForgeResult:
        return await self.client._invoke(
            ForgeInvocation(
                kind="join",
                workflow_id=self.workflow_id,
                aggregator_id=aggregator_id,
                branches=self.branches,
            )
        )


class ForgeClient:
    def __init__(self, base_url: str = "http://127.0.0.1:8787", token: Optional[str] = None) -> None:
        self.base_url = base_url
        self.token = token

    def workflow(self, workflow_id: str) -> ForgeWorkflow:
        return ForgeWorkflow(self, workflow_id)

    async def _invoke(self, payload: ForgeInvocation) -> ForgeResult:
        return ForgeResult(
            base_url=self.base_url,
            token_present=self.token is not None,
            payload=payload,
        )


async def _gather(branches: List[Awaitable[Any]]) -> List[Any]:
    return list(await asyncio.gather(*branches))
