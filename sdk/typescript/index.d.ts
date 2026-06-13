export type ForgeInvocationKind =
  | "workflow"
  | "node"
  | "resume"
  | "join";

export interface ForgeClientOptions {
  baseUrl?: string;
  token?: string | null;
}

export interface ForgeInvocation {
  kind: ForgeInvocationKind;
  workflow_id: string;
  node_id?: string;
  resume_id?: string;
  aggregator_id?: string;
  input?: unknown;
  branches?: unknown[];
}

export interface ForgeResult {
  baseUrl: string;
  tokenPresent: boolean;
  payload: ForgeInvocation;
}

export declare class ForgeNode {
  constructor(client: ForgeClient, workflowId: string, nodeId: string);
  run(input?: unknown): Promise<ForgeResult>;
}

export declare class ForgeWorkflow {
  constructor(client: ForgeClient, workflowId: string);
  node(nodeId: string): ForgeNode;
  run(input?: unknown): Promise<ForgeResult>;
  resume(resumeId: string): Promise<ForgeResult>;
  subworkflow(workflowId: string): Promise<ForgeWorkflow>;
  parallel(
    workflows: Promise<unknown>[],
  ): Promise<{ join(aggregatorId: string): Promise<ForgeResult> }>;
}

export declare class ForgeClient {
  constructor(options?: ForgeClientOptions);
  workflow(workflowId: string): ForgeWorkflow;
}
