use crate::interactive::{
    build_interactive_home_with_options, route_interactive_input, InteractiveHomeOptions,
    InteractiveHomeReport,
};
use crate::storage::{ForgeStore, GlobalEventWrite};
use anyhow::Result;
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{
    self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
    Event, KeyCode, KeyEventKind, KeyModifiers,
};
use crossterm::style::{Attribute, Color, Print, ResetColor, SetAttribute, SetForegroundColor};
use crossterm::terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, queue};
use serde::Serialize;
use serde_json::json;
use std::io::{stdout, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const FORGE_TUI_SCHEMA_VERSION: &str = "forge.tui.opencode_orchestrator.v1";
const FORGE_TUI_ORCHESTRATOR_SCHEMA_VERSION: &str = "forge.tui.orchestrator.v1";
const MAX_ACTIVITY_LINES: usize = 80;

#[derive(Debug, Clone, Serialize)]
pub struct ForgeTuiReport {
    pub schema_version: String,
    pub status: String,
    pub layout: String,
    pub orchestrator: ForgeTuiOrchestrator,
    pub renderer_strategy: ForgeTuiRendererStrategy,
    pub prompt: ForgeTuiPrompt,
    pub shell: ForgeTuiShell,
    pub status_bar: ForgeTuiStatusBar,
    pub visualizations: Vec<ForgeTuiVisualization>,
    pub capabilities: Vec<ForgeTuiCapability>,
    pub quick_commands: Vec<String>,
    pub session_tabs: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForgeTuiOrchestrator {
    pub schema_version: String,
    pub default_interaction: String,
    pub decision_policy: String,
    pub plan_mode: String,
    pub build_mode: String,
    pub agent_model: String,
    pub node_agent_routing: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForgeTuiRendererStrategy {
    pub schema_version: String,
    pub current_backend: String,
    pub target_backend: String,
    pub ecosystem_sources: Vec<String>,
    pub rust_native_candidates: Vec<String>,
    pub bridge_candidates: Vec<String>,
    pub component_system_candidates: Vec<String>,
    pub create_tui_template_family: String,
    pub next_step: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForgeTuiPrompt {
    pub placeholder: String,
    pub submit_hint: String,
    pub command_hint: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForgeTuiShell {
    pub enabled: bool,
    pub prefix: String,
    pub toggle: String,
    pub audit_event_kind: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForgeTuiStatusBar {
    pub workflows: usize,
    pub active_runs: usize,
    pub events: usize,
    pub addons: usize,
    pub capabilities: usize,
    pub ready_handoffs: usize,
    pub pending_approvals: usize,
    pub estimated_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForgeTuiVisualization {
    pub id: String,
    pub title: String,
    pub command: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ForgeTuiCapability {
    pub id: String,
    pub title: String,
    pub command: String,
    pub summary: String,
}

struct ForgeTuiRuntimeState {
    input: String,
    shell_mode: bool,
    focus: String,
    activity: Vec<String>,
}

#[derive(Debug, Clone, Copy)]
struct PaneFrame {
    x: u16,
    y: u16,
    width: u16,
    height: u16,
}

pub fn build_forge_tui(
    store: &ForgeStore,
    project_root: Option<PathBuf>,
) -> Result<ForgeTuiReport> {
    let home = build_interactive_home_with_options(store, InteractiveHomeOptions { project_root })?;
    Ok(forge_tui_from_home(&home))
}

fn forge_tui_from_home(home: &InteractiveHomeReport) -> ForgeTuiReport {
    let d = &home.dashboard;
    let status_bar = ForgeTuiStatusBar {
        workflows: d.task_board_panel.workflow_count,
        active_runs: d.active_runs,
        events: d.event_panel.total_event_count,
        addons: d.addon_capability_panel.enabled_addon_count,
        capabilities: d.addon_capability_panel.capability_count,
        ready_handoffs: d.task_board_panel.ready_handoffs,
        pending_approvals: d.pending_approvals,
        estimated_cost_usd: d.cost_panel.estimated_task_cost_total_usd,
    };

    let session_tabs = if d.shell_entrypoints.is_empty() {
        vec!["forge".to_string()]
    } else {
        d.shell_entrypoints.iter().take(4).cloned().collect()
    };

    let orchestrator = ForgeTuiOrchestrator {
        schema_version: FORGE_TUI_ORCHESTRATOR_SCHEMA_VERSION.to_string(),
        default_interaction: "conversation_with_forge_orchestrator".to_string(),
        decision_policy: "direct_answer_or_create_workflow".to_string(),
        plan_mode: "forge_workflow".to_string(),
        build_mode: "forge_workflow".to_string(),
        agent_model: "agents_and_subagents_are_workflows_or_nodes".to_string(),
        node_agent_routing: "per_node_agent_allowed".to_string(),
        summary:
            "normal input talks to the Forge orchestrator; slash commands configure the surface"
                .to_string(),
    };

    ForgeTuiReport {
        schema_version: FORGE_TUI_SCHEMA_VERSION.to_string(),
        status: "forge_tui_ready".to_string(),
        layout: "opencode_style_orchestrator_first_tui".to_string(),
        orchestrator,
        renderer_strategy: ForgeTuiRendererStrategy {
            schema_version: "forge.tui.renderer_strategy.v1".to_string(),
            current_backend: "rust_crossterm_fullscreen_fallback".to_string(),
            target_backend: "opentui_native_core_bridge_or_incremental_rust_port".to_string(),
            ecosystem_sources: vec![
                "anomalyco/opentui native Zig renderer and TypeScript bindings".to_string(),
                "msmps/create-tui OpenTUI starter templates".to_string(),
                "msmps/awesome-opentui ecosystem map and testing references".to_string(),
                "msmps/opentui-ui component, dialog, toast and styled-slot patterns".to_string(),
            ],
            rust_native_candidates: vec![
                "Forge state projection and orchestrator routing".to_string(),
                "Terminal lifecycle fallback for environments without Bun/Zig".to_string(),
                "Key command routing and shell command audit events".to_string(),
                "Workflow, agent, subagent and node-agent data contracts".to_string(),
                "Forge-owned component metadata, slots, states and variant contracts".to_string(),
            ],
            bridge_candidates: vec![
                "OpenTUI Zig/C ABI renderer and optimized buffer".to_string(),
                "Yoga-style flex layout and component tree reconciliation".to_string(),
                "Unicode grapheme width, text buffer, editor, diff and markdown renderables"
                    .to_string(),
                "Mouse hit grid, split-footer scrollback, remote feed and frame statistics"
                    .to_string(),
                "create-tui Core/React/Solid bootstrap templates for generated renderer surfaces"
                    .to_string(),
            ],
            component_system_candidates: vec![
                "opentui-ui-style slot metadata for panels, badges, prompts and controls"
                    .to_string(),
                "state selectors for focused, selected, disabled, loading and errored UI states"
                    .to_string(),
                "dialog manager pattern for confirmations, handoffs and approvals".to_string(),
                "toast pattern for operation feedback, shell events and workflow notifications"
                    .to_string(),
            ],
            create_tui_template_family:
                "core template for Forge-owned runtime; React/Solid only for external Addon renderer prototypes"
                    .to_string(),
            next_step:
                "prototype a Forge TUI renderer adapter that can choose crossterm fallback or OpenTUI bridge"
                    .to_string(),
        },
        prompt: ForgeTuiPrompt {
            placeholder: "Talk to the Forge orchestrator; use /commands for configuration"
                .to_string(),
            submit_hint:
                "Enter sends the message to the orchestrator; it answers or creates a workflow"
                    .to_string(),
            command_hint: "!<cmd> runs local shell; ! toggles shell mode".to_string(),
        },
        shell: ForgeTuiShell {
            enabled: true,
            prefix: "!".to_string(),
            toggle: "!".to_string(),
            audit_event_kind: "forge_tui_shell_command".to_string(),
        },
        visualizations: vec![
            visualization(
                "workflows",
                "Workflows",
                "/workflows",
                format!(
                    "{} workflows, {} active runs, {} ready handoffs",
                    status_bar.workflows, status_bar.active_runs, status_bar.ready_handoffs
                ),
            ),
            visualization(
                "agents",
                "Agents",
                "/agents",
                format!(
                    "orchestrator plus {} visible execution session(s)",
                    session_tabs.len()
                ),
            ),
            visualization(
                "subagents",
                "Subagents",
                "/subagents",
                "subagents are Forge workflows, child workflows or DAG nodes".to_string(),
            ),
            visualization(
                "node_agents",
                "Node agents",
                "/nodes",
                "each workflow node can carry its own agent/brain routing".to_string(),
            ),
        ],
        capabilities: vec![
            capability(
                "orchestrator",
                "Orchestrator",
                "/orchestrator",
                "decides direct answer or workflow".to_string(),
            ),
            capability(
                "plan_workflows",
                "Plan workflows",
                "/plan",
                "plan is a Forge workflow".to_string(),
            ),
            capability(
                "build_workflows",
                "Build workflows",
                "/build",
                "build is a Forge workflow".to_string(),
            ),
            capability(
                "workflows",
                "Workflows",
                "/workflows",
                format!("{} total", status_bar.workflows),
            ),
            capability(
                "agents",
                "Agents",
                "/agents",
                "agents are workflows or nodes".to_string(),
            ),
            capability(
                "subagents",
                "Subagents",
                "/subagents",
                "subagents are child workflows or nodes".to_string(),
            ),
            capability(
                "node_agents",
                "Node agents",
                "/nodes",
                "per-node agent allowed".to_string(),
            ),
            capability(
                "events",
                "Events",
                "/events",
                format!("{} recorded", status_bar.events),
            ),
            capability(
                "addons",
                "Addons",
                "/addons",
                format!("{} enabled", status_bar.addons),
            ),
            capability(
                "costs",
                "Costs",
                "/costs",
                format!("estimated ${:.4}", status_bar.estimated_cost_usd),
            ),
            capability(
                "handoffs",
                "Handoffs",
                "/handoffs",
                format!("{} ready", status_bar.ready_handoffs),
            ),
            capability(
                "approvals",
                "Approvals",
                "/approvals",
                format!("{} pending", status_bar.pending_approvals),
            ),
            capability(
                "core_boundary",
                "Core boundary",
                "/boundary",
                d.core_boundary_panel.status.clone(),
            ),
            capability(
                "shells",
                "Shells",
                "/shells",
                format!("{} visible", session_tabs.len()),
            ),
            capability(
                "command_palette",
                "Command Palette",
                "/commands",
                format!("{} actions", d.command_palette_panel.entry_count),
            ),
        ],
        quick_commands: vec![
            "/orchestrator".to_string(),
            "/workflows".to_string(),
            "/agents".to_string(),
            "/subagents".to_string(),
            "/nodes".to_string(),
            "/events".to_string(),
            "/addons".to_string(),
            "/costs".to_string(),
            "/approvals".to_string(),
            "/config".to_string(),
            "/boundary".to_string(),
            "/shells".to_string(),
            "/commands".to_string(),
            "/help".to_string(),
        ],
        session_tabs,
        notes: vec![
            "OpenCode-inspired terminal surface; Forge owns orchestration and state.".to_string(),
            "Legacy detailed panels remain under `forge interactive ...`.".to_string(),
        ],
        status_bar,
    }
}

fn capability(id: &str, title: &str, command: &str, summary: String) -> ForgeTuiCapability {
    ForgeTuiCapability {
        id: id.to_string(),
        title: title.to_string(),
        command: command.to_string(),
        summary,
    }
}

fn visualization(id: &str, title: &str, command: &str, summary: String) -> ForgeTuiVisualization {
    ForgeTuiVisualization {
        id: id.to_string(),
        title: title.to_string(),
        command: command.to_string(),
        summary,
    }
}

pub fn render_forge_tui(report: &ForgeTuiReport) -> String {
    let visualizations = report
        .visualizations
        .iter()
        .map(|visualization| visualization.title.as_str())
        .collect::<Vec<_>>()
        .join(" | ");
    let sessions = if report.session_tabs.is_empty() {
        "none".to_string()
    } else {
        report.session_tabs.join(" | ")
    };
    let quick = report.quick_commands.join(" ");

    format!(
        "forge\n\
         Forge TUI - orchestrator-first OpenCode-style\n\
         Prompt: {placeholder}\n\
         Orchestrator: default conversation; direct answer or workflow; plan/build are Forge workflows\n\
         Agents: agents and subagents are Forge workflows or nodes; each node can have a specific agent\n\
         Renderer: {current_backend} -> {target_backend}\n\
         Shell: !<cmd> runs locally; ! toggles shell mode; commands are audited as {audit_kind}\n\
         Status: workflows {workflows} | active runs {active_runs} | events {events} | addons {addons} | caps {capabilities}\n\
         Flow: handoffs {handoffs} | approvals {approvals} | cost ${cost:.4}\n\
         Visualizations: {visualizations}\n\
         Sessions: {sessions}\n\
         Quick: {quick}\n\
         Legacy panels: forge interactive home | forge interactive guided-cockpit | forge interactive task-board\n\
         forge> ",
        placeholder = report.prompt.placeholder,
        current_backend = report.renderer_strategy.current_backend,
        target_backend = report.renderer_strategy.target_backend,
        audit_kind = report.shell.audit_event_kind,
        workflows = report.status_bar.workflows,
        active_runs = report.status_bar.active_runs,
        events = report.status_bar.events,
        addons = report.status_bar.addons,
        capabilities = report.status_bar.capabilities,
        handoffs = report.status_bar.ready_handoffs,
        approvals = report.status_bar.pending_approvals,
        cost = report.status_bar.estimated_cost_usd,
        visualizations = visualizations,
        sessions = sessions,
        quick = quick,
    )
}

pub fn run_forge_tui(store_path: &Path, project_root: Option<PathBuf>) -> Result<i32> {
    let store = ForgeStore::open(store_path)?;
    let report = build_forge_tui(&store, project_root.clone())?;

    if !std::io::stdin().is_terminal() {
        println!("{}", render_forge_tui(&report));
        return Ok(0);
    }

    run_fullscreen_tui(&store, project_root, report)
}

fn run_fullscreen_tui(
    store: &ForgeStore,
    project_root: Option<PathBuf>,
    mut report: ForgeTuiReport,
) -> Result<i32> {
    let mut stdout = stdout();
    terminal::enable_raw_mode()?;
    execute!(
        stdout,
        EnterAlternateScreen,
        EnableBracketedPaste,
        EnableMouseCapture,
        Hide
    )?;
    let _guard = ForgeTuiTerminalGuard;

    let mut state = ForgeTuiRuntimeState {
        input: String::new(),
        shell_mode: false,
        focus: "orchestrator".to_string(),
        activity: vec![
            "Forge TUI ready: orchestrator decides direct answer or workflow.".to_string(),
            "Talk to the Forge orchestrator. Use /commands for configuration.".to_string(),
            "Views: /workflows /events /addons /approvals".to_string(),
            "Shell: !<cmd> or ! to toggle shell mode.".to_string(),
        ],
    };

    render_fullscreen(&mut stdout, &report, &state)?;
    loop {
        if !event::poll(Duration::from_millis(250))? {
            continue;
        }

        match event::read()? {
            Event::Key(key) if key.kind != KeyEventKind::Release => match key.code {
                KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    break;
                }
                KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    break;
                }
                KeyCode::Char('j' | 'm') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    let input = state.input.trim().to_string();
                    state.input.clear();
                    if handle_tui_submit(
                        store,
                        &mut report,
                        project_root.clone(),
                        &mut state,
                        &input,
                    )? {
                        break;
                    }
                }
                KeyCode::Char(ch) => {
                    state.input.push(ch);
                }
                KeyCode::Backspace => {
                    state.input.pop();
                }
                KeyCode::Enter => {
                    let input = state.input.trim().to_string();
                    state.input.clear();
                    if handle_tui_submit(
                        store,
                        &mut report,
                        project_root.clone(),
                        &mut state,
                        &input,
                    )? {
                        break;
                    }
                }
                KeyCode::Esc => {
                    state.focus = "orchestrator".to_string();
                    state.input.clear();
                    push_activity(&mut state, "Focus: orchestrator".to_string());
                }
                KeyCode::Tab => {
                    cycle_focus(&mut state);
                }
                _ => {}
            },
            Event::Paste(text) => state.input.push_str(&text),
            Event::Resize(_, _) => {}
            _ => {}
        }
        render_fullscreen(&mut stdout, &report, &state)?;
    }

    drop(_guard);
    println!("goodbye");
    Ok(0)
}

struct ForgeTuiTerminalGuard;

impl Drop for ForgeTuiTerminalGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let mut stdout = stdout();
        let _ = execute!(
            stdout,
            Show,
            DisableMouseCapture,
            DisableBracketedPaste,
            ResetColor,
            LeaveAlternateScreen
        );
    }
}

fn handle_tui_submit(
    store: &ForgeStore,
    report: &mut ForgeTuiReport,
    project_root: Option<PathBuf>,
    state: &mut ForgeTuiRuntimeState,
    input: &str,
) -> Result<bool> {
    if input.is_empty() {
        return Ok(false);
    }
    push_activity(state, format!("{} {}", prompt_label(state), input));

    if matches!(input, "q" | "quit" | "exit" | "/quit" | "/exit") {
        push_activity(state, "goodbye".to_string());
        return Ok(true);
    }

    if input == "!" {
        state.shell_mode = !state.shell_mode;
        push_activity(
            state,
            if state.shell_mode {
                "Shell mode enabled".to_string()
            } else {
                "Shell mode disabled".to_string()
            },
        );
        return Ok(false);
    }

    if state.shell_mode {
        for line in dispatch_shell_command(store, input)? {
            push_activity(state, line);
        }
        *report = build_forge_tui(store, project_root)?;
        return Ok(false);
    }

    if let Some(command) = input.strip_prefix('!').map(str::trim) {
        if command.is_empty() {
            state.shell_mode = true;
            push_activity(state, "Shell mode enabled".to_string());
        } else {
            for line in dispatch_shell_command(store, command)? {
                push_activity(state, line);
            }
            *report = build_forge_tui(store, project_root)?;
        }
        return Ok(false);
    }

    if let Some(lines) = dispatch_forge_tui_command(report, input) {
        for line in lines {
            push_activity(state, line);
        }
        if let Some(command) = input.strip_prefix('/') {
            state.focus = command
                .split_whitespace()
                .next()
                .unwrap_or("orchestrator")
                .replace('-', "_");
        }
        return Ok(false);
    }

    let route = route_interactive_input(store, input, "forge_tui")?;
    push_activity(state, format!("Orchestrator: {}", route.routing_decision));
    if let Some(answer) = route.answer {
        push_activity(state, answer);
    }
    if route.workflow_created {
        if let Some(run_id) = route.run_id {
            push_activity(state, format!("Run ID: {run_id}"));
        }
        if let Some(workflow_id) = route.workflow_id {
            push_activity(state, format!("Workflow ID: {workflow_id}"));
        }
    }
    *report = build_forge_tui(store, project_root)?;
    Ok(false)
}

fn dispatch_forge_tui_command(report: &ForgeTuiReport, input: &str) -> Option<Vec<String>> {
    match input {
        "/help" | "/commands" => Some(vec![
            "Commands: /orchestrator /workflows /agents /subagents /nodes /events".to_string(),
            "Commands: /addons /costs /approvals /config /boundary".to_string(),
            "Commands: /shells /commands /help".to_string(),
            "Shell: !<cmd> or ! to toggle shell mode".to_string(),
        ]),
        "/orchestrator" => Some(vec![
            "Orchestrator: default conversation".to_string(),
            "The orchestrator decides direct answer or workflow.".to_string(),
            "Plan and build are Forge workflows.".to_string(),
        ]),
        "/workflows" => Some(vec![format!(
            "Workflows: {} total; active runs {}",
            report.status_bar.workflows, report.status_bar.active_runs
        )]),
        "/agents" => Some(vec![
            "Agents: agents are Forge workflows or DAG nodes.".to_string(),
            "Each node can bind a specific agent/brain routing contract.".to_string(),
        ]),
        "/subagents" => Some(vec![
            "Subagents: subagents are child workflows, subflows or nodes.".to_string(),
            "They remain visible in the workflow graph instead of hidden model calls.".to_string(),
        ]),
        "/nodes" | "/node-agents" => Some(vec![
            "Node agents: per-node agent routing allowed.".to_string(),
            "Use forge workflow update-node-brain to mutate routing without stopping a run."
                .to_string(),
        ]),
        "/events" => Some(vec![format!(
            "Events: {} recorded",
            report.status_bar.events
        )]),
        "/addons" => Some(vec![format!(
            "Addons: {} enabled; capabilities {}",
            report.status_bar.addons, report.status_bar.capabilities
        )]),
        "/costs" => Some(vec![format!(
            "Costs: estimated ${:.4}",
            report.status_bar.estimated_cost_usd
        )]),
        "/handoffs" => Some(vec![format!(
            "Handoffs: {} ready",
            report.status_bar.ready_handoffs
        )]),
        "/approvals" => Some(vec![format!(
            "Approvals: {} pending",
            report.status_bar.pending_approvals
        )]),
        "/config" => Some(vec![
            "Configuration: use slash commands for views and behavior.".to_string(),
            "Normal text stays conversation with the Forge orchestrator.".to_string(),
        ]),
        "/boundary" | "/core-boundary" => report
            .capabilities
            .iter()
            .find(|capability| capability.id == "core_boundary")
            .map(|capability| vec![format!("Core boundary: {}", capability.summary)]),
        "/shells" => Some(vec![format!(
            "Shell sessions: {}",
            report.session_tabs.join(" | ")
        )]),
        _ => None,
    }
}

fn dispatch_shell_command(store: &ForgeStore, command: &str) -> Result<Vec<String>> {
    let mut lines = vec![format!("Shell command: {command}")];
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
    let output = Command::new(shell).arg("-lc").arg(command).output()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    for line in stdout.lines() {
        if !line.trim().is_empty() {
            lines.push(line.to_string());
        }
    }
    for line in stderr.lines() {
        if !line.trim().is_empty() {
            lines.push(line.to_string());
        }
    }
    let exit_code = output.status.code().unwrap_or_default();
    lines.push(format!("Shell exit: {exit_code}"));
    record_shell_event(store, command, exit_code)?;
    Ok(lines)
}

fn record_shell_event(store: &ForgeStore, command: &str, exit_code: i32) -> Result<()> {
    let data = json!({
        "schema_version": "forge.tui.shell_command.v1",
        "command": command,
        "exit_code": exit_code,
    });
    let tenant = json!({});
    store.record_global_event(GlobalEventWrite {
        source: "forge.tui",
        source_id: "forge-tui",
        workflow_id: None,
        kind: "forge_tui_shell_command",
        origin: "forge_tui",
        status: if exit_code == 0 {
            "completed"
        } else {
            "failed"
        },
        data: &data,
        tenant_context: &tenant,
    })?;
    Ok(())
}

fn render_fullscreen(
    stdout: &mut std::io::Stdout,
    report: &ForgeTuiReport,
    state: &ForgeTuiRuntimeState,
) -> Result<()> {
    let (width, height) = terminal::size().unwrap_or((100, 32));
    let width = width.max(72);
    let height = height.max(24);
    queue!(stdout, MoveTo(0, 0), Clear(ClearType::All))?;

    draw_header(stdout, report, width)?;
    let body_top = 4;
    let input_height = 4;
    let body_height = height.saturating_sub(body_top + input_height + 1);

    if width >= 110 && body_height >= 18 {
        let left_width = width * 33 / 100;
        let right_width = width.saturating_sub(left_width + 2);
        let row_height = body_height / 2;
        draw_box(
            stdout,
            PaneFrame {
                x: 0,
                y: body_top,
                width: left_width,
                height: row_height,
            },
            "ORCHESTRATOR",
            &orchestrator_lines(report),
            Color::Cyan,
        )?;
        draw_box(
            stdout,
            PaneFrame {
                x: left_width + 1,
                y: body_top,
                width: right_width,
                height: row_height,
            },
            "WORKFLOWS",
            &workflow_lines(report),
            Color::Green,
        )?;
        draw_box(
            stdout,
            PaneFrame {
                x: 0,
                y: body_top + row_height + 1,
                width: left_width,
                height: body_height.saturating_sub(row_height + 1),
            },
            "AGENTS / SUBAGENTS / NODE AGENTS",
            &agent_lines(report),
            Color::Magenta,
        )?;
        let activity_height = body_height.saturating_sub(row_height + 1);
        draw_box(
            stdout,
            PaneFrame {
                x: left_width + 1,
                y: body_top + row_height + 1,
                width: right_width,
                height: activity_height,
            },
            "ACTIVITY",
            &activity_lines(state, activity_height.saturating_sub(2) as usize),
            Color::Yellow,
        )?;
    } else {
        let pane_height = body_height / 4;
        draw_box(
            stdout,
            PaneFrame {
                x: 0,
                y: body_top,
                width,
                height: pane_height,
            },
            "ORCHESTRATOR",
            &orchestrator_lines(report),
            Color::Cyan,
        )?;
        draw_box(
            stdout,
            PaneFrame {
                x: 0,
                y: body_top + pane_height,
                width,
                height: pane_height,
            },
            "WORKFLOWS",
            &workflow_lines(report),
            Color::Green,
        )?;
        draw_box(
            stdout,
            PaneFrame {
                x: 0,
                y: body_top + pane_height * 2,
                width,
                height: pane_height,
            },
            "AGENTS / SUBAGENTS / NODE AGENTS",
            &agent_lines(report),
            Color::Magenta,
        )?;
        let activity_height = body_height.saturating_sub(pane_height * 3);
        draw_box(
            stdout,
            PaneFrame {
                x: 0,
                y: body_top + pane_height * 3,
                width,
                height: activity_height,
            },
            "ACTIVITY",
            &activity_lines(state, activity_height.saturating_sub(2) as usize),
            Color::Yellow,
        )?;
    }

    draw_input(
        stdout,
        report,
        state,
        width,
        height.saturating_sub(input_height),
    )?;
    stdout.flush()?;
    Ok(())
}

fn draw_header(stdout: &mut std::io::Stdout, report: &ForgeTuiReport, width: u16) -> Result<()> {
    queue!(
        stdout,
        SetForegroundColor(Color::Cyan),
        SetAttribute(Attribute::Bold),
        MoveTo(0, 0),
        Print(pad(
            " FORGE  Forge TUI  Orchestrator-first  OpenCode-style shell + Forge workflows",
            width as usize
        )),
        ResetColor,
        SetAttribute(Attribute::Reset)
    )?;
    queue!(
        stdout,
        SetForegroundColor(Color::DarkGrey),
        MoveTo(0, 1),
        Print(pad(
            &format!(
                " Workflows {} | Active {} | Events {} | Addons {} | Approvals {} | Cost ${:.4}",
                report.status_bar.workflows,
                report.status_bar.active_runs,
                report.status_bar.events,
                report.status_bar.addons,
                report.status_bar.pending_approvals,
                report.status_bar.estimated_cost_usd
            ),
            width as usize
        )),
        ResetColor
    )?;
    queue!(
        stdout,
        SetForegroundColor(Color::DarkGrey),
        MoveTo(0, 2),
        Print(pad(
            " /orchestrator /workflows /agents /subagents /nodes /events /addons /costs /approvals /config  |  !<cmd> shell",
            width as usize
        )),
        ResetColor
    )?;
    Ok(())
}

fn draw_input(
    stdout: &mut std::io::Stdout,
    report: &ForgeTuiReport,
    state: &ForgeTuiRuntimeState,
    width: u16,
    y: u16,
) -> Result<()> {
    let prompt = prompt_label(state);
    draw_box(
        stdout,
        PaneFrame {
            x: 0,
            y,
            width,
            height: 4,
        },
        if state.shell_mode {
            "SHELL INPUT"
        } else {
            "ORCHESTRATOR INPUT"
        },
        &[
            report.prompt.placeholder.clone(),
            format!("{prompt} {}", state.input),
        ],
        if state.shell_mode {
            Color::Yellow
        } else {
            Color::Cyan
        },
    )
}

fn draw_box(
    stdout: &mut std::io::Stdout,
    frame: PaneFrame,
    title: &str,
    lines: &[String],
    color: Color,
) -> Result<()> {
    let PaneFrame {
        x,
        y,
        width,
        height,
    } = frame;
    if width < 4 || height < 3 {
        return Ok(());
    }
    let inner = width.saturating_sub(2) as usize;
    queue!(
        stdout,
        SetForegroundColor(color),
        MoveTo(x, y),
        Print(format!("╭{}╮", "─".repeat(inner))),
        MoveTo(x + 2, y),
        SetAttribute(Attribute::Bold),
        Print(format!(" {} ", clip(title, inner.saturating_sub(2)))),
        SetAttribute(Attribute::Reset),
        ResetColor
    )?;

    let content_height = height.saturating_sub(2);
    for row in 0..content_height {
        let line = lines.get(row as usize).cloned().unwrap_or_default();
        queue!(
            stdout,
            SetForegroundColor(color),
            MoveTo(x, y + 1 + row),
            Print("│"),
            ResetColor,
            Print(pad(&line, inner)),
            SetForegroundColor(color),
            Print("│"),
            ResetColor
        )?;
    }
    queue!(
        stdout,
        SetForegroundColor(color),
        MoveTo(x, y + height - 1),
        Print(format!("╰{}╯", "─".repeat(inner))),
        ResetColor
    )?;
    Ok(())
}

fn orchestrator_lines(report: &ForgeTuiReport) -> Vec<String> {
    vec![
        "Default: conversation with Forge orchestrator".to_string(),
        "Policy: orchestrator decides direct answer or workflow".to_string(),
        "Plan: Forge workflow".to_string(),
        "Build: Forge workflow".to_string(),
        format!("Agent model: {}", report.orchestrator.agent_model),
        format!("Node routing: {}", report.orchestrator.node_agent_routing),
        format!(
            "Renderer: {} -> {}",
            report.renderer_strategy.current_backend, report.renderer_strategy.target_backend
        ),
        "Slash commands configure; normal text stays conversational".to_string(),
    ]
}

fn workflow_lines(report: &ForgeTuiReport) -> Vec<String> {
    vec![
        format!("Workflows: {} total", report.status_bar.workflows),
        format!("Active runs: {}", report.status_bar.active_runs),
        format!("Ready handoffs: {}", report.status_bar.ready_handoffs),
        format!(
            "Pending approvals: {} (/approvals)",
            report.status_bar.pending_approvals
        ),
        format!("Events/schedules: {}", report.status_bar.events),
        "Plan/build/agents are visible workflow objects".to_string(),
        "Use /workflows for the current graph summary".to_string(),
    ]
}

fn agent_lines(report: &ForgeTuiReport) -> Vec<String> {
    vec![
        "Agents: workflows or nodes, not hidden direct model calls".to_string(),
        "Subagents: child workflows, subflows or DAG nodes".to_string(),
        "Node agents: each node can have a specific agent".to_string(),
        "Brains: Codex/OpenCode/Gemini/Claude are execution resources".to_string(),
        format!("Sessions: {}", report.session_tabs.join(" | ")),
        "Hot-swap routing stays Forge-owned".to_string(),
        "Use /agents, /subagents or /nodes".to_string(),
    ]
}

fn activity_lines(state: &ForgeTuiRuntimeState, max_lines: usize) -> Vec<String> {
    let start = state.activity.len().saturating_sub(max_lines.max(1));
    state.activity[start..].to_vec()
}

fn cycle_focus(state: &mut ForgeTuiRuntimeState) {
    state.focus = match state.focus.as_str() {
        "orchestrator" => "workflows",
        "workflows" => "agents",
        "agents" => "activity",
        _ => "orchestrator",
    }
    .to_string();
    push_activity(state, format!("Focus: {}", state.focus));
}

fn push_activity(state: &mut ForgeTuiRuntimeState, line: String) {
    state.activity.push(line);
    if state.activity.len() > MAX_ACTIVITY_LINES {
        let remove_count = state.activity.len() - MAX_ACTIVITY_LINES;
        state.activity.drain(0..remove_count);
    }
}

fn prompt_label(state: &ForgeTuiRuntimeState) -> &'static str {
    if state.shell_mode {
        "forge-shell>"
    } else {
        "forge>"
    }
}

fn pad(text: &str, width: usize) -> String {
    let clipped = clip(text, width);
    let count = clipped.chars().count();
    if count >= width {
        clipped
    } else {
        format!("{clipped}{}", " ".repeat(width - count))
    }
}

fn clip(text: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let mut clipped = text.chars().take(width).collect::<String>();
    if text.chars().count() > width && width >= 1 {
        clipped.pop();
        clipped.push('…');
    }
    clipped
}
