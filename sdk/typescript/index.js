class FoundryNode {
  constructor(client, workflowId, nodeId) {
    this.client = client;
    this.workflowId = workflowId;
    this.nodeId = nodeId;
  }

  async run(input = {}) {
    return this.client._invoke({
      kind: "node",
      workflow_id: this.workflowId,
      node_id: this.nodeId,
      input,
    });
  }
}

class FoundryWorkflow {
  constructor(client, workflowId) {
    this.client = client;
    this.workflowId = workflowId;
  }

  node(nodeId) {
    return new FoundryNode(this.client, this.workflowId, nodeId);
  }

  async run(input = {}) {
    return this.client._invoke({
      kind: "workflow",
      workflow_id: this.workflowId,
      input,
    });
  }

  async resume(resumeId) {
    return this.client._invoke({
      kind: "resume",
      workflow_id: this.workflowId,
      resume_id: resumeId,
    });
  }

  async subworkflow(workflowId) {
    return new FoundryWorkflow(this.client, workflowId);
  }

  async parallel(workflows) {
    const results = await Promise.all(workflows);
    return {
      join: async (aggregatorId) =>
        this.client._invoke({
          kind: "join",
          workflow_id: this.workflowId,
          aggregator_id: aggregatorId,
          branches: results,
        }),
    };
  }
}

class FoundryClient {
  constructor(options = {}) {
    this.baseUrl = options.baseUrl || "http://127.0.0.1:8787";
    this.token = options.token || null;
  }

  workflow(workflowId) {
    return new FoundryWorkflow(this, workflowId);
  }

  async _invoke(payload) {
    return {
      baseUrl: this.baseUrl,
      tokenPresent: Boolean(this.token),
      payload,
    };
  }
}

module.exports = {
  FoundryClient,
  FoundryWorkflow,
  FoundryNode,
  // Deprecated Forge-era aliases retained only for the 0.6.x migration cycle. // foundry-brand-allow: legacy-compat
  ForgeClient: FoundryClient, // foundry-brand-allow: legacy-compat
  ForgeWorkflow: FoundryWorkflow, // foundry-brand-allow: legacy-compat
  ForgeNode: FoundryNode, // foundry-brand-allow: legacy-compat
};
