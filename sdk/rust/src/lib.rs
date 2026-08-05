//! Foundry Rust SDK.

use std::future::Future;
use std::pin::Pin;

#[derive(Debug, Clone)]
pub struct Invocation {
    pub kind: String,
    pub workflow_id: String,
    pub node_id: Option<String>,
    pub resume_id: Option<String>,
    pub aggregator_id: Option<String>,
    pub input: Option<String>,
    pub branches: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ResultEnvelope {
    pub base_url: String,
    pub token_present: bool,
    pub payload: Invocation,
}

#[derive(Debug, Clone)]
pub struct Client {
    pub base_url: String,
    pub token: Option<String>,
}

impl Client {
    pub fn new(base_url: impl Into<String>, token: Option<String>) -> Self {
        let base_url = base_url.into();
        Self {
            base_url: if base_url.is_empty() {
                "http://127.0.0.1:8787".to_string()
            } else {
                base_url
            },
            token,
        }
    }

    pub fn workflow(&self, workflow_id: impl Into<String>) -> Workflow {
        Workflow {
            client: self.clone(),
            workflow_id: workflow_id.into(),
        }
    }

    async fn invoke(&self, payload: Invocation) -> ResultEnvelope {
        ResultEnvelope {
            base_url: self.base_url.clone(),
            token_present: self.token.is_some(),
            payload,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Workflow {
    client: Client,
    workflow_id: String,
}

impl Workflow {
    pub fn node(&self, node_id: impl Into<String>) -> Node {
        Node {
            client: self.client.clone(),
            workflow_id: self.workflow_id.clone(),
            node_id: node_id.into(),
        }
    }

    pub fn subworkflow(&self, workflow_id: impl Into<String>) -> Workflow {
        self.client.workflow(workflow_id)
    }

    pub fn run(
        &self,
        input: Option<String>,
    ) -> Pin<Box<dyn Future<Output = ResultEnvelope> + Send + '_>> {
        let client = self.client.clone();
        let workflow_id = self.workflow_id.clone();
        Box::pin(async move {
            client
                .invoke(Invocation {
                    kind: "workflow".to_string(),
                    workflow_id,
                    node_id: None,
                    resume_id: None,
                    aggregator_id: None,
                    input,
                    branches: vec![],
                })
                .await
        })
    }

    pub fn resume(
        &self,
        resume_id: impl Into<String>,
    ) -> Pin<Box<dyn Future<Output = ResultEnvelope> + Send + '_>> {
        let client = self.client.clone();
        let workflow_id = self.workflow_id.clone();
        let resume_id = resume_id.into();
        Box::pin(async move {
            client
                .invoke(Invocation {
                    kind: "resume".to_string(),
                    workflow_id,
                    node_id: None,
                    resume_id: Some(resume_id),
                    aggregator_id: None,
                    input: None,
                    branches: vec![],
                })
                .await
        })
    }

    pub fn parallel(&self, branches: Vec<String>) -> ParallelJoin {
        ParallelJoin {
            client: self.client.clone(),
            workflow_id: self.workflow_id.clone(),
            branches,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Node {
    client: Client,
    workflow_id: String,
    node_id: String,
}

impl Node {
    pub fn run(
        &self,
        input: Option<String>,
    ) -> Pin<Box<dyn Future<Output = ResultEnvelope> + Send + '_>> {
        let client = self.client.clone();
        let workflow_id = self.workflow_id.clone();
        let node_id = self.node_id.clone();
        Box::pin(async move {
            client
                .invoke(Invocation {
                    kind: "node".to_string(),
                    workflow_id,
                    node_id: Some(node_id),
                    resume_id: None,
                    aggregator_id: None,
                    input,
                    branches: vec![],
                })
                .await
        })
    }
}

#[derive(Debug, Clone)]
pub struct ParallelJoin {
    client: Client,
    workflow_id: String,
    branches: Vec<String>,
}

impl ParallelJoin {
    pub fn join(
        &self,
        aggregator_id: impl Into<String>,
    ) -> Pin<Box<dyn Future<Output = ResultEnvelope> + Send + '_>> {
        let client = self.client.clone();
        let workflow_id = self.workflow_id.clone();
        let branches = self.branches.clone();
        let aggregator_id = aggregator_id.into();
        Box::pin(async move {
            client
                .invoke(Invocation {
                    kind: "join".to_string(),
                    workflow_id,
                    node_id: None,
                    resume_id: None,
                    aggregator_id: Some(aggregator_id),
                    input: None,
                    branches,
                })
                .await
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_api_is_constructible() {
        let client = Client::new("", None);
        let workflow = client.workflow("demo");
        let _ = workflow.node("step");
        assert_eq!(client.base_url, "http://127.0.0.1:8787");
    }
}
