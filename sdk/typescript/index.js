class ForgeNode {
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

class ForgeWorkflow {
  constructor(client, workflowId) {
    this.client = client;
    this.workflowId = workflowId;
  }

  node(nodeId) {
    return new ForgeNode(this.client, this.workflowId, nodeId);
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
    return new ForgeWorkflow(this.client, workflowId);
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

class ForgeClient {
  constructor(options = {}) {
    this.baseUrl = options.baseUrl || "http://127.0.0.1:8787";
    this.token = options.token || null;
  }

  workflow(workflowId) {
    return new ForgeWorkflow(this, workflowId);
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
  ForgeClient,
  ForgeWorkflow,
  ForgeNode,
};
