package foundry

import "context"

type Invocation struct {
	Kind         string
	WorkflowID   string
	NodeID       string
	ResumeID     string
	AggregatorID string
	Input        any
	Branches     []any
}

type Result struct {
	BaseURL      string
	TokenPresent bool
	Payload      Invocation
}

type Client struct {
	BaseURL string
	Token   string
}

func NewClient(baseURL, token string) *Client {
	if baseURL == "" {
		baseURL = "http://127.0.0.1:8787"
	}
	return &Client{BaseURL: baseURL, Token: token}
}

func (c *Client) Workflow(workflowID string) *Workflow {
	return &Workflow{client: c, workflowID: workflowID}
}

type Workflow struct {
	client     *Client
	workflowID string
}

func (w *Workflow) Node(nodeID string) *Node {
	return &Node{client: w.client, workflowID: w.workflowID, nodeID: nodeID}
}

func (w *Workflow) Run(ctx context.Context, input any) (Result, error) {
	return w.client.invoke(Invocation{Kind: "workflow", WorkflowID: w.workflowID, Input: input})
}

func (w *Workflow) Resume(ctx context.Context, resumeID string) (Result, error) {
	return w.client.invoke(Invocation{Kind: "resume", WorkflowID: w.workflowID, ResumeID: resumeID})
}

func (w *Workflow) Subworkflow(workflowID string) *Workflow {
	return &Workflow{client: w.client, workflowID: workflowID}
}

type Node struct {
	client     *Client
	workflowID string
	nodeID     string
}

func (n *Node) Run(ctx context.Context, input any) (Result, error) {
	return n.client.invoke(Invocation{Kind: "node", WorkflowID: n.workflowID, NodeID: n.nodeID, Input: input})
}

type ParallelJoin struct {
	client     *Client
	workflowID string
	branches   []any
}

func (w *Workflow) Parallel(branches ...any) *ParallelJoin {
	return &ParallelJoin{client: w.client, workflowID: w.workflowID, branches: branches}
}

func (p *ParallelJoin) Join(ctx context.Context, aggregatorID string) (Result, error) {
	return p.client.invoke(Invocation{Kind: "join", WorkflowID: p.workflowID, AggregatorID: aggregatorID, Branches: p.branches})
}

func (c *Client) invoke(payload Invocation) (Result, error) {
	return Result{BaseURL: c.BaseURL, TokenPresent: c.Token != "", Payload: payload}, nil
}
