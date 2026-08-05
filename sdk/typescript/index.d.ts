export type FoundryInvocationKind =
  | "workflow"
  | "node"
  | "resume"
  | "join";

export interface FoundryClientOptions {
  baseUrl?: string;
  token?: string | null;
}

export interface FoundryInvocation {
  kind: FoundryInvocationKind;
  workflow_id: string;
  node_id?: string;
  resume_id?: string;
  aggregator_id?: string;
  input?: unknown;
  branches?: unknown[];
}

export interface FoundryResult {
  baseUrl: string;
  tokenPresent: boolean;
  payload: FoundryInvocation;
}

export declare class FoundryNode {
  constructor(client: FoundryClient, workflowId: string, nodeId: string);
  run(input?: unknown): Promise<FoundryResult>;
}

export declare class FoundryWorkflow {
  constructor(client: FoundryClient, workflowId: string);
  node(nodeId: string): FoundryNode;
  run(input?: unknown): Promise<FoundryResult>;
  resume(resumeId: string): Promise<FoundryResult>;
  subworkflow(workflowId: string): Promise<FoundryWorkflow>;
  parallel(
    workflows: Promise<unknown>[],
  ): Promise<{ join(aggregatorId: string): Promise<FoundryResult> }>;
}

export declare class FoundryClient {
  constructor(options?: FoundryClientOptions);
  workflow(workflowId: string): FoundryWorkflow;
}

/** @deprecated Use FoundryInvocationKind during the 0.6.x migration window. */
export type ForgeInvocationKind = FoundryInvocationKind; // foundry-brand-allow: legacy-compat
/** @deprecated Use FoundryClientOptions during the 0.6.x migration window. */
export type ForgeClientOptions = FoundryClientOptions; // foundry-brand-allow: legacy-compat
/** @deprecated Use FoundryInvocation during the 0.6.x migration window. */
export type ForgeInvocation = FoundryInvocation; // foundry-brand-allow: legacy-compat
/** @deprecated Use FoundryResult during the 0.6.x migration window. */
export type ForgeResult = FoundryResult; // foundry-brand-allow: legacy-compat
/** @deprecated Use the Foundry-prefixed classes. */
export {
  FoundryClient as ForgeClient, // foundry-brand-allow: legacy-compat
  FoundryNode as ForgeNode, // foundry-brand-allow: legacy-compat
  FoundryWorkflow as ForgeWorkflow, // foundry-brand-allow: legacy-compat
};
