use std::cmp::min;
use std::error::Error;
use std::future::Future;
use std::io;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use distributed_diagnostics::api_clients::postgres::run_state_store::{
    PostgresRunStateStore, PostgresRunStateStoreConfig,
};
use distributed_diagnostics::config;
use distributed_diagnostics::observability::ObservabilityRuntime;
use distributed_diagnostics::orchestrator::orchestrator::{Orchestrator, RunOutcome};
use distributed_diagnostics::orchestrator::run_repository::{RunListItem, RunRepository};
use distributed_diagnostics::orchestrator::run_state::model::{
    RunId, RunIteration, RunIterationStatus, RunState, RunStatus, StepKind, StepRecord,
    StepResultEnvelope,
};
use distributed_diagnostics::orchestrator::transition_policy::DiagnosticLoopTransitionPolicy;
use distributed_diagnostics::shared_types::{
    AdequacyAssessment, CardHydrationOutput, DiagnosticResponse, HypothesisEvidenceSource,
    HypothesisStatus, ObservationBoundaryResolution, ObservationExtractionOutput, UserRequest,
};
use futures::task::noop_waker;
use distributed_diagnostics::startup::{self, StartupError};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{Frame, Terminal};

#[derive(Parser)]
#[command(about = "Distributed diagnostics TUI")]
struct Cli {
    #[arg(long)]
    config: PathBuf,

    #[arg(long)]
    ingest_config: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Runs,
    Details,
    Input,
}

type PendingSubmission = Pin<Box<dyn Future<Output = Result<RunOutcome, String>>>>;

struct App {
    run_repository: Arc<RunRepository>,
    orchestrator: Arc<Orchestrator<DiagnosticLoopTransitionPolicy>>,
    runs: Vec<RunListItem>,
    selected_index: usize,
    selected_run: Option<RunState>,
    details_scroll: u16,
    focus: Focus,
    draft_input: String,
    status_message: String,
    pending_submission: Option<PendingSubmission>,
    should_quit: bool,
}

impl App {
    fn new(
        run_repository: Arc<RunRepository>,
        orchestrator: Arc<Orchestrator<DiagnosticLoopTransitionPolicy>>,
    ) -> Self {
        Self {
            run_repository,
            orchestrator,
            runs: Vec::new(),
            selected_index: 0,
            selected_run: None,
            details_scroll: 0,
            focus: Focus::Runs,
            draft_input: String::new(),
            status_message: "Tab to navigate. Up/Down works on the focused panel.".to_string(),
            pending_submission: None,
            should_quit: false,
        }
    }

    async fn initialize(&mut self) -> Result<(), String> {
        self.refresh_runs(None).await
    }

    fn is_new_run_selected(&self) -> bool {
        self.selected_index == 0
    }

    fn selected_run_id(&self) -> Option<RunId> {
        if self.selected_index == 0 {
            None
        } else {
            self.runs.get(self.selected_index - 1).map(|run| run.run_id)
        }
    }

    fn total_run_rows(&self) -> usize {
        self.runs.len() + 1
    }

    fn is_busy(&self) -> bool {
        self.pending_submission.is_some()
    }

    fn cycle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Runs => Focus::Details,
            Focus::Details => Focus::Input,
            Focus::Input => Focus::Runs,
        };
    }

    async fn refresh_runs(&mut self, preferred_run_id: Option<RunId>) -> Result<(), String> {
        let mut runs = self
            .run_repository
            .list_runs()
            .await
            .map_err(|e| format!("Failed to load runs: {e}"))?;
        runs.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
        self.runs = runs;

        if let Some(run_id) = preferred_run_id {
            self.selected_index = self
                .runs
                .iter()
                .position(|run| run.run_id == run_id)
                .map(|index| index + 1)
                .unwrap_or(0);
        } else if self.selected_index >= self.total_run_rows() {
            self.selected_index = self.total_run_rows().saturating_sub(1);
        } else if self.selected_index == 0 && !self.runs.is_empty() {
            self.selected_index = 1;
        }

        self.load_selected_run().await
    }

    async fn load_selected_run(&mut self) -> Result<(), String> {
        self.details_scroll = 0;
        self.selected_run = match self.selected_run_id() {
            Some(run_id) => self
                .run_repository
                .load_run(run_id)
                .await
                .map_err(|e| format!("Failed to load run details: {e}"))?,
            None => None,
        };
        Ok(())
    }

    async fn move_run_selection(&mut self, delta: isize) -> Result<(), String> {
        let row_count = self.total_run_rows();
        if row_count == 0 {
            self.selected_index = 0;
            self.selected_run = None;
            return Ok(());
        }

        let current = self.selected_index as isize;
        let next = (current + delta).clamp(0, row_count.saturating_sub(1) as isize);
        if next as usize != self.selected_index {
            self.selected_index = next as usize;
            self.load_selected_run().await?;
        }
        Ok(())
    }

    async fn select_first(&mut self) -> Result<(), String> {
        self.selected_index = 0;
        self.load_selected_run().await
    }

    async fn select_last(&mut self) -> Result<(), String> {
        self.selected_index = self.total_run_rows().saturating_sub(1);
        self.load_selected_run().await
    }

    fn begin_new_run(&mut self) {
        self.selected_index = 0;
        self.selected_run = None;
        self.details_scroll = 0;
        self.focus = Focus::Input;
        self.draft_input.clear();
        self.status_message = "Type the first query for a new run and press Enter.".to_string();
    }

    fn begin_input_for_selected_run(&mut self) {
        if self.is_new_run_selected() {
            self.begin_new_run();
            return;
        }
        self.focus = Focus::Input;
        self.draft_input.clear();
        self.status_message = if self
            .selected_run
            .as_ref()
            .map(|run| run.status == RunStatus::WaitingForUser)
            .unwrap_or(false)
        {
            "This run is waiting for more data. Type the answer and press Enter.".to_string()
        } else {
            "Type a new observation for this run and press Enter.".to_string()
        };
    }

    fn scroll_details(&mut self, delta: i16) {
        self.details_scroll = if delta.is_negative() {
            self.details_scroll.saturating_sub(delta.unsigned_abs())
        } else {
            self.details_scroll.saturating_add(delta as u16)
        };
    }

    fn start_submission(&mut self) -> Result<(), String> {
        if self.is_busy() {
            self.status_message =
                "A query is already running. Please wait before submitting again.".to_string();
            return Ok(());
        }

        let input = self.draft_input.trim().to_string();
        if input.is_empty() {
            self.status_message = "Input is empty. Type a query or observation first.".to_string();
            return Ok(());
        }

        let selected_run_id = self.selected_run_id();
        let orchestrator = Arc::clone(&self.orchestrator);
        self.draft_input.clear();
        self.focus = Focus::Runs;
        self.status_message =
            "Running query... please wait. The interface will update automatically.".to_string();
        self.pending_submission = Some(Box::pin(async move {
            match selected_run_id {
                Some(run_id) => orchestrator
                    .resume_with_input(
                        run_id,
                        UserRequest {
                            query: input,
                            golden_question: None,
                        },
                    )
                    .await
                    .map_err(|e| format!("Failed to resume run: {e}")),
                None => orchestrator
                    .run(UserRequest {
                        query: input,
                        golden_question: None,
                    })
                    .await
                    .map_err(|e| format!("Failed to start run: {e}")),
            }
        }));
        Ok(())
    }

    async fn poll_background_work(&mut self) -> Result<(), String> {
        let Some(future) = self.pending_submission.as_mut() else {
            return Ok(());
        };
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        let poll_result = future.as_mut().poll(&mut cx);
        let outcome = match poll_result {
            Poll::Pending => return Ok(()),
            Poll::Ready(outcome) => outcome?,
        };
        self.pending_submission = None;

        let affected_run_id = match &outcome {
            RunOutcome::Finished { run_id, .. }
            | RunOutcome::WaitingForUser { run_id, .. }
            | RunOutcome::Failed { run_id, .. } => *run_id,
        };
        self.status_message = outcome_status_message(&outcome);
        self.refresh_runs(Some(affected_run_id)).await
    }

    async fn handle_key(&mut self, key: KeyEvent) -> Result<(), String> {
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return Ok(());
        }

        if key.code == KeyCode::Tab {
            self.cycle_focus();
            return Ok(());
        }

        match self.focus {
            Focus::Runs => self.handle_runs_key(key).await,
            Focus::Details => self.handle_details_key(key).await,
            Focus::Input => self.handle_input_key(key).await,
        }
    }

    async fn handle_runs_key(&mut self, key: KeyEvent) -> Result<(), String> {
        match key.code {
            KeyCode::Char('q') => {
                self.should_quit = true;
                Ok(())
            }
            KeyCode::Down => self.move_run_selection(1).await,
            KeyCode::Up => self.move_run_selection(-1).await,
            KeyCode::Home => self.select_first().await,
            KeyCode::End => self.select_last().await,
            KeyCode::Enter => {
                if self.is_new_run_selected() {
                    self.begin_new_run();
                } else {
                    self.focus = Focus::Details;
                    self.status_message =
                        "Run details are focused. Use Up/Down to scroll iterations.".to_string();
                }
                Ok(())
            }
            KeyCode::Char('n') => {
                self.begin_new_run();
                Ok(())
            }
            KeyCode::Char('i') => {
                self.begin_input_for_selected_run();
                Ok(())
            }
            KeyCode::Char('r') => self.refresh_runs(self.selected_run_id()).await,
            _ => Ok(()),
        }
    }

    async fn handle_details_key(&mut self, key: KeyEvent) -> Result<(), String> {
        match key.code {
            KeyCode::Char('q') => {
                self.should_quit = true;
                Ok(())
            }
            KeyCode::Down => {
                self.scroll_details(1);
                Ok(())
            }
            KeyCode::Up => {
                self.scroll_details(-1);
                Ok(())
            }
            KeyCode::Home => {
                self.details_scroll = 0;
                Ok(())
            }
            KeyCode::End => {
                self.details_scroll = u16::MAX / 4;
                Ok(())
            }
            KeyCode::Char('i') => {
                self.begin_input_for_selected_run();
                Ok(())
            }
            KeyCode::Char('r') => self.refresh_runs(self.selected_run_id()).await,
            _ => Ok(()),
        }
    }

    async fn handle_input_key(&mut self, key: KeyEvent) -> Result<(), String> {
        match key.code {
            KeyCode::Esc => {
                self.focus = Focus::Runs;
                self.status_message = "Returned to run list.".to_string();
                Ok(())
            }
            KeyCode::Backspace => {
                if !self.is_busy() {
                    self.draft_input.pop();
                }
                Ok(())
            }
            KeyCode::Enter => self.start_submission(),
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if !self.is_busy() {
                    self.draft_input.clear();
                }
                Ok(())
            }
            KeyCode::Up => {
                self.focus = Focus::Details;
                Ok(())
            }
            KeyCode::Down => {
                self.focus = Focus::Runs;
                Ok(())
            }
            KeyCode::Delete => {
                if !self.is_busy() {
                    self.draft_input.clear();
                }
                Ok(())
            }
            KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                if !self.is_busy() {
                    self.draft_input.push(ch);
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

struct TerminalSession;

impl TerminalSession {
    fn enter() -> Result<Self, Box<dyn Error>> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        Ok(Self)
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli = Cli::parse();
    let ingest_config = resolve_ingest_config(&cli.config, cli.ingest_config)?;
    let settings = config::load(&cli.config, &ingest_config)?;

    let _observability = ObservabilityRuntime::initialize(&settings.observability)?;

    let orchestrator = Arc::new(startup::build_orchestrator(&settings).await?);
    let run_state_store = PostgresRunStateStore::new(PostgresRunStateStoreConfig {
        postgres_url: settings.postgres.url.clone(),
    })
    .await
    .map_err(StartupError::RunStateStore)?;
    let run_repository = Arc::new(RunRepository::new(run_state_store));

    let _session = TerminalSession::enter()?;
    let backend = CrosstermBackend::new(io::stdout());
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(run_repository, orchestrator);
    if let Err(message) = app.initialize().await {
        app.status_message = message;
    }

    loop {
        terminal.draw(|frame| render(frame, &app))?;

        if app.should_quit {
            break;
        }

        if let Err(message) = app.poll_background_work().await {
            app.status_message = message;
        }

        if event::poll(Duration::from_millis(200))? {
            if let Event::Key(key) = event::read()? {
                if let Err(message) = app.handle_key(key).await {
                    app.status_message = message;
                }
            }
        }
    }

    terminal.show_cursor()?;
    Ok(())
}

fn render(frame: &mut Frame<'_>, app: &App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(5),
            Constraint::Length(2),
        ])
        .split(frame.area());

    render_header(frame, app, root[0]);

    let content = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(34), Constraint::Percentage(66)])
        .split(root[1]);

    render_runs_list(frame, app, content[0]);
    render_details(frame, app, content[1]);
    render_input(frame, app, root[2]);
    render_footer(frame, app, root[3]);
}

fn render_header(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let header = Paragraph::new(Line::from(vec![
        Span::styled(
            "Distributed Diagnostics",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            match app.focus {
                Focus::Runs => "Focus: runs",
                Focus::Details => "Focus: details",
                Focus::Input => "Focus: input",
            },
            Style::default().fg(Color::Yellow),
        ),
        Span::raw("  "),
        Span::styled("Tab to navigate", Style::default().fg(Color::Green)),
        if app.is_busy() {
            Span::styled("  Running...", Style::default().fg(Color::Magenta))
        } else {
            Span::raw("")
        },
    ]))
    .block(Block::default().borders(Borders::ALL).title("Overview"));
    frame.render_widget(header, area);
}

fn render_runs_list(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let mut items = Vec::with_capacity(app.runs.len() + 1);
    items.push(ListItem::new(Line::from(vec![
        Span::styled("+ ", Style::default().fg(Color::Green)),
        Span::styled("Start new run", Style::default().add_modifier(Modifier::BOLD)),
    ])));

    let content_width = area.width.saturating_sub(4) as usize;
    for run in &app.runs {
        let continuation_indent = "    ";
        let time_label = run.updated_at.format("%m-%d %H:%M").to_string();
        let prefix = format!("[{}] {}  ", run_status_short(run.status), time_label);
        let wrapped = wrap_with_ellipsis(
            &run.initial_user_query,
            content_width,
            &prefix,
            continuation_indent.chars().count(),
            2,
        );
        let mut lines = Vec::new();
        for (index, segment) in wrapped.into_iter().enumerate() {
            if index == 0 {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("[{}] ", run_status_short(run.status)),
                        run_status_style(run.status),
                    ),
                    Span::styled(time_label.clone(), Style::default().fg(Color::DarkGray)),
                    Span::raw("  "),
                    Span::raw(segment),
                ]));
            } else {
                lines.push(Line::from(format!("{continuation_indent}{segment}")));
            }
        }
        items.push(ListItem::new(lines));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Runs")
                .border_style(panel_border_style(app.focus == Focus::Runs)),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(21, 34, 56))
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol(">> ");

    let mut state = ListState::default();
    state.select(Some(app.selected_index));
    frame.render_stateful_widget(list, area, &mut state);
}

fn render_details(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Run Details")
        .border_style(panel_border_style(app.focus == Focus::Details));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let text = if let Some(run) = &app.selected_run {
        build_run_details(run, inner.width as usize)
    } else {
        Text::from(vec![
            Line::from("Start a new run or pick an existing run from the list."),
            Line::from(""),
            Line::from("The details panel shows:"),
            Line::from("- problem understanding"),
            Line::from("- run id, status, iteration count"),
            Line::from("- last signal quality"),
            Line::from("- primary and alternative similar incidents"),
            Line::from("- current top hypothesis"),
            Line::from("- raw user input"),
            Line::from("- resolved with prior context"),
            Line::from("- signal quality, reason, follow-up questions"),
            Line::from("- alternative interpretation"),
            Line::from("- hypotheses with source, check, and result interpretation"),
        ])
    };

    let details = Paragraph::new(text).scroll((app.details_scroll, 0));
    frame.render_widget(details, inner);
}

fn render_input(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let title = if app.is_busy() {
        "Input: request is running..."
    } else if app.is_new_run_selected() {
        "Input: new run query (Enter to start)"
    } else if app
        .selected_run
        .as_ref()
        .map(|run| run.status == RunStatus::WaitingForUser)
        .unwrap_or(false)
    {
        "Input: answer requested by the run (Enter to submit)"
    } else {
        "Input: add observation (Enter to submit)"
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(panel_border_style(app.focus == Focus::Input));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let input_text = if app.is_busy() {
        Text::from(Line::from(vec![Span::styled(
            "Query is running. Please wait for the result before typing again.",
            Style::default().fg(Color::DarkGray),
        )]))
    } else if app.draft_input.is_empty() {
        if app.is_new_run_selected() {
            Text::from(Line::from(vec![Span::styled(
                "Type the first query for a new run, then press Enter.",
                Style::default().fg(Color::DarkGray),
            )]))
        } else if app
            .selected_run
            .as_ref()
            .map(|run| run.status == RunStatus::WaitingForUser)
            .unwrap_or(false)
        {
            Text::from(Line::from(vec![Span::styled(
                "Type the missing information or answer, then press Enter.",
                Style::default().fg(Color::DarkGray),
            )]))
        } else {
            Text::from(Line::from(vec![Span::styled(
                "Type a new observation for this run, then press Enter.",
                Style::default().fg(Color::DarkGray),
            )]))
        }
    } else {
        Text::from(app.draft_input.as_str())
    };

    let input = Paragraph::new(input_text)
        .style(if app.is_busy() {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
        })
        .wrap(Wrap { trim: false });
    frame.render_widget(input, inner);

    if app.focus == Focus::Input && !app.is_busy() {
        let cursor_x =
            inner.x + min(app.draft_input.chars().count() as u16, inner.width.saturating_sub(1));
        frame.set_cursor_position((cursor_x, inner.y));
    }
}

fn render_footer(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let footer = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(56), Constraint::Percentage(44)])
        .split(area);

    let help = Paragraph::new(Line::from(
        "Tab navigate  Up/Down act on panel  Enter inspect/submit  n new  i input  r refresh  q quit",
    ))
    .block(Block::default().borders(Borders::TOP).title("Keys"));
    frame.render_widget(help, footer[0]);

    let status = Paragraph::new(app.status_message.as_str())
        .style(Style::default().fg(Color::Yellow))
        .block(Block::default().borders(Borders::TOP).title("Status"));
    frame.render_widget(status, footer[1]);
}

fn build_run_details(run: &RunState, width: usize) -> Text<'static> {
    let mut lines = Vec::new();
    let current = latest_response(run);

    lines.push(styled_title_line("Problem understanding"));
    push_wrapped_plain(
        &mut lines,
        &current
            .map(|response| response.problem_understanding.clone())
            .unwrap_or_else(|| "Not available yet.".to_string()),
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        width,
        0,
        0,
    );
    push_wrapped_header_kv(&mut lines, "Run ID", &run.run_id.0.to_string(), width);
    push_wrapped_header_kv(&mut lines, "Status", &run.status.to_string(), width);
    push_wrapped_header_kv(&mut lines, "Iterations", &run.iterations.len().to_string(), width);
    push_wrapped_header_kv(
        &mut lines,
        "Last signal quality",
        &latest_signal_quality(run).unwrap_or("Not available".to_string()),
        width,
    );
    push_wrapped_header_kv(
        &mut lines,
        "Current top hypothesis",
        &current_top_hypothesis(run).unwrap_or("Not available".to_string()),
        width,
    );
    lines.push(Line::from(""));

    lines.push(styled_title_line("Iterations"));
    for (index, iteration) in run.iterations.iter().enumerate() {
        let previous_response = if index == 0 {
            None
        } else {
            iteration_response(&run.iterations[index - 1])
        };
        append_iteration_lines(&mut lines, iteration, index + 1, width, previous_response);
    }

    Text::from(lines)
}

fn styled_title_line(title: &str) -> Line<'static> {
    Line::from(vec![Span::styled(
        title.to_string(),
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
    )])
}

fn append_iteration_lines(
    lines: &mut Vec<Line<'static>>,
    iteration: &RunIteration,
    index: usize,
    width: usize,
    previous_response: Option<&DiagnosticResponse>,
) {
    lines.push(Line::from(vec![
        Span::styled(
            format!("Iteration {index}"),
            iteration_status_style(iteration.status)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        ),
        Span::raw(format!("  Status: {}", iteration.status)),
    ]));

    if index == 1 {
        if let Some(raw_input) = iteration_user_input(iteration) {
            push_wrapped_labeled_field(lines, "  Raw user input", raw_input, width, 2);
        } else {
            push_wrapped_labeled_field(lines, "  Raw user input", "Not available.", width, 2);
        }
    } else {
        lines.push(dark_label_line("  Observation"));
        if let Some(raw_input) = iteration_user_input(iteration) {
            push_wrapped_plain_subfield(lines, "    Raw", raw_input, width, 4);
        } else {
            push_wrapped_plain_subfield(lines, "    Raw", "Not available.", width, 4);
        }
        if let Some(value) = initial_or_resolved_value(iteration, index) {
            push_wrapped_plain_subfield(
                lines,
                "    Resolved with prior context",
                &value,
                width,
                4,
            );
        }
    }

    if let Some(assessment) = iteration_adequacy(iteration) {
        push_wrapped_labeled_field(
            lines,
            "  Signal quality",
            &format!("{:?}", assessment.status),
            width,
            2,
        );
        push_wrapped_plain_subfield(lines, "    Reason", &assessment.summary_reason, width, 4);
        if assessment.follow_up_questions.is_empty() {
            push_wrapped_plain_subfield(lines, "    Follow-up questions", "None.", width, 4);
        } else {
            for question in &assessment.follow_up_questions {
                push_wrapped_plain_subfield(lines, "    Follow-up question", question, width, 4);
            }
        }
        if let Some(cards) = iteration_card_hydration(iteration) {
            if let Some(primary) = cards.primary.as_ref() {
                push_wrapped_labeled_field(
                    lines,
                    "  Primary similar incident",
                    &primary.title,
                    width,
                    2,
                );
            }

            if !cards.alternatives.is_empty() {
                lines.push(dark_label_line("  Alternative similar incidents"));
                for (idx, card) in cards.alternatives.iter().enumerate() {
                    push_wrapped_plain(
                        lines,
                        &format!("{}. {}", idx + 1, card.title),
                        Style::default(),
                        width,
                        4,
                        4,
                    );
                }
            }
        }
    } else {
        push_wrapped_labeled_field(lines, "  Signal quality", "Not available.", width, 2);
    }

    if let Some(response) = iteration_response(iteration) {
        push_wrapped_labeled_field(
            lines,
            "  Problem understanding",
            &response.problem_understanding,
            width,
            2,
        );
        if let Some(alternative) = response.competing_interpretation.as_deref() {
            push_wrapped_labeled_field(
                lines,
                "  Alternative interpretation",
                alternative,
                width,
                2,
            );
        }
        if response.hypotheses.is_empty() {
            lines.push(dark_label_line("  Hypothesis"));
            push_wrapped_plain(lines, "Not available.", Style::default(), width, 2, 2);
        } else {
            lines.push(dark_label_line("  Hypothesis"));
            for (i, hypothesis) in response.hypotheses.iter().enumerate() {
                push_wrapped_plain(
                    lines,
                    &format!(
                        "{}. [{} / {:?}] {} [Source: {}]",
                        i + 1,
                        hypothesis_status_label(&hypothesis.status),
                        confidence_with_indicator(
                            hypothesis,
                            previous_response.and_then(|response| {
                                response
                                    .hypotheses
                                    .iter()
                                    .find(|previous| previous.id == hypothesis.id)
                            }),
                        ),
                        hypothesis.text,
                        hypothesis_source_label(hypothesis.source),
                    ),
                    Style::default(),
                    width,
                    4,
                    4,
                );
            }
        }
        lines.push(Line::from(""));
        push_wrapped_underlined_labeled_field(lines, "  Check", &response.first_check, width, 2);
        push_wrapped_plain_subfield(
            lines,
            "    Supports primary if",
            &response.result_interpretation.supports_primary_if,
            width,
            4,
        );
        push_wrapped_plain_subfield(
            lines,
            "    Supports alternative if",
            &response.result_interpretation.supports_competing_if,
            width,
            4,
        );
    } else {
        push_wrapped_labeled_field(lines, "  Problem understanding", "Not available.", width, 2);
        lines.push(dark_label_line("  Hypothesis"));
        push_wrapped_plain(lines, "Not available.", Style::default(), width, 2, 2);
        lines.push(Line::from(""));
        push_wrapped_underlined_labeled_field(lines, "  Check", "Not available.", width, 2);
    }

    if let Some(extraction) = iteration_observation_extraction(iteration) {
        for question in &extraction.missing_context_questions {
            push_wrapped_plain_subfield(lines, "    Follow-up question", question, width, 4);
        }
    }

    for record in &iteration.step_records {
        if let StepRecord::Finished(finished) = record {
            if let Err(error) = &finished.result {
                push_wrapped_plain(
                    lines,
                    &format!("Error at {}: {}", finished.step, error),
                    Style::default(),
                    width,
                    2,
                    2,
                );
                break;
            }
        }
    }

    lines.push(Line::from(""));
}

fn latest_response(run: &RunState) -> Option<&DiagnosticResponse> {
    run.iterations.iter().rev().find_map(iteration_response)
}

fn latest_signal_quality(run: &RunState) -> Option<String> {
    run.iterations
        .iter()
        .rev()
        .find_map(iteration_adequacy)
        .map(|assessment| format!("{:?}", assessment.status))
}

fn current_top_hypothesis(run: &RunState) -> Option<String> {
    latest_response(run).and_then(|response| {
        response.hypotheses.first().map(|hypothesis| {
            format!(
                "[{} / {:?}] {}",
                hypothesis_status_label(&hypothesis.status),
                hypothesis.confidence,
                hypothesis.text
            )
        })
    })
}

fn iteration_response(iteration: &RunIteration) -> Option<&DiagnosticResponse> {
    iteration.step_records.iter().rev().find_map(|record| match record {
        StepRecord::Finished(finished)
            if finished.step == StepKind::ResponseValidationAndNormalization =>
        {
            match &finished.result {
                Ok(StepResultEnvelope::ResponseValidationAndNormalization(output)) => {
                    Some(&output.response)
                }
                _ => None,
            }
        }
        _ => None,
    })
}

fn iteration_user_input(iteration: &RunIteration) -> Option<&str> {
    iteration.step_records.iter().find_map(|record| match record {
        StepRecord::Finished(finished) if finished.step == StepKind::UserInputReceived => {
            match &finished.result {
                Ok(StepResultEnvelope::UserInputReceived(request)) => Some(request.query.as_str()),
                _ => None,
            }
        }
        _ => None,
    })
}

fn initial_or_resolved_value(iteration: &RunIteration, index: usize) -> Option<String> {
    if index == 1 {
        return None;
    }

    iteration.step_records.iter().rev().find_map(|record| match record {
        StepRecord::Finished(finished) if finished.step == StepKind::ObservationExtraction => {
            match &finished.result {
                Ok(StepResultEnvelope::ObservationExtraction(output)) => {
                    Some(output.resolved_observation.text.clone())
                }
                _ => None,
            }
        }
        StepRecord::Finished(finished)
            if finished.step == StepKind::ObservationBoundaryResolver =>
        {
            match &finished.result {
                Ok(StepResultEnvelope::ObservationBoundaryResolver(output)) => {
                    match &output.resolution {
                        ObservationBoundaryResolution::Supported(observation) => {
                            Some(observation.text.clone())
                        }
                        ObservationBoundaryResolution::Unsupported => {
                            Some(output.normalized_user_input.clone())
                        }
                    }
                }
                _ => None,
            }
        }
        _ => None,
    })
}

fn dark_label_line(label: &str) -> Line<'static> {
    let indent_width = label.chars().take_while(|c| c.is_whitespace()).count();
    let trimmed = label.trim();
    Line::from(vec![
        Span::raw(" ".repeat(indent_width)),
        Span::styled(
            format!("{trimmed}:"),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
    ])
}

fn labeled_field(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label}: "),
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
        ),
        Span::raw(value.to_string()),
    ])
}

fn header_kv_line(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            label.to_string(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        ),
        Span::raw(": "),
        Span::styled(value.to_string(), Style::default().fg(Color::White)),
    ])
}

fn plain_subfield(label: &str, value: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{label}: "),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(value.to_string()),
    ])
}

fn push_wrapped_header_kv(lines: &mut Vec<Line<'static>>, label: &str, value: &str, width: usize) {
    let prefix = format!("{label}: ");
    let parts = wrap_with_indent(value, width, prefix.chars().count(), 2);
    if parts.is_empty() {
        lines.push(header_kv_line(label, value));
        return;
    }
    lines.push(Line::from(vec![
        Span::styled(
            label.to_string(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        ),
        Span::raw(": "),
        Span::styled(parts[0].clone(), Style::default().fg(Color::White)),
    ]));
    for part in parts.into_iter().skip(1) {
        lines.push(Line::from(format!("  {part}")));
    }
}

fn push_wrapped_labeled_field(
    lines: &mut Vec<Line<'static>>,
    label: &str,
    value: &str,
    width: usize,
    continuation_indent: usize,
) {
    let prefix = format!("{label}: ");
    let parts = wrap_with_indent(value, width, prefix.chars().count(), continuation_indent);
    if parts.is_empty() {
        lines.push(labeled_field(label, value));
        return;
    }
    lines.push(Line::from(vec![
        Span::styled(
            prefix,
            Style::default().fg(Color::DarkGray).add_modifier(Modifier::BOLD),
        ),
        Span::raw(parts[0].clone()),
    ]));
    for part in parts.into_iter().skip(1) {
        lines.push(Line::from(format!("{}{}", " ".repeat(continuation_indent), part)));
    }
}

fn push_wrapped_underlined_labeled_field(
    lines: &mut Vec<Line<'static>>,
    label: &str,
    value: &str,
    width: usize,
    continuation_indent: usize,
) {
    let prefix = format!("{label}: ");
    let parts = wrap_with_indent(value, width, prefix.chars().count(), continuation_indent);
    if parts.is_empty() {
        let indent_width = label.chars().take_while(|c| c.is_whitespace()).count();
        let trimmed = label.trim();
        lines.push(Line::from(vec![
            Span::raw(" ".repeat(indent_width)),
            Span::styled(
                format!("{trimmed}:"),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
            ),
            Span::raw(" "),
            Span::raw(value.to_string()),
        ]));
        return;
    }

    let indent_width = label.chars().take_while(|c| c.is_whitespace()).count();
    let trimmed = label.trim();
    lines.push(Line::from(vec![
        Span::raw(" ".repeat(indent_width)),
        Span::styled(
            format!("{trimmed}:"),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED),
        ),
        Span::raw(" "),
        Span::raw(parts[0].clone()),
    ]));
    for part in parts.into_iter().skip(1) {
        lines.push(Line::from(format!("{}{}", " ".repeat(continuation_indent), part)));
    }
}

fn push_wrapped_plain_subfield(
    lines: &mut Vec<Line<'static>>,
    label: &str,
    value: &str,
    width: usize,
    continuation_indent: usize,
) {
    let prefix = format!("{label}: ");
    let parts = wrap_with_indent(value, width, prefix.chars().count(), continuation_indent);
    if parts.is_empty() {
        lines.push(plain_subfield(label, value));
        return;
    }
    lines.push(Line::from(vec![
        Span::styled(
            prefix,
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(parts[0].clone()),
    ]));
    for part in parts.into_iter().skip(1) {
        lines.push(Line::from(format!("{}{}", " ".repeat(continuation_indent), part)));
    }
}

fn push_wrapped_plain(
    lines: &mut Vec<Line<'static>>,
    value: &str,
    style: Style,
    width: usize,
    first_line_indent: usize,
    continuation_indent: usize,
) {
    let parts = wrap_with_indent(value, width, first_line_indent, continuation_indent);
    if parts.is_empty() {
        lines.push(Line::from(""));
        return;
    }
    lines.push(Line::from(vec![Span::styled(
        format!("{}{}", " ".repeat(first_line_indent), parts[0]),
        style,
    )]));
    for part in parts.into_iter().skip(1) {
        lines.push(Line::from(format!("{}{}", " ".repeat(continuation_indent), part)));
    }
}

fn wrap_with_indent(
    text: &str,
    total_width: usize,
    first_prefix_width: usize,
    continuation_indent_width: usize,
) -> Vec<String> {
    let clean = text.trim();
    if clean.is_empty() {
        return Vec::new();
    }

    let widths = [
        total_width.saturating_sub(first_prefix_width),
        total_width.saturating_sub(continuation_indent_width),
    ];
    let chars: Vec<char> = clean.chars().collect();
    let mut idx = 0usize;
    let mut lines = Vec::new();
    let mut is_first = true;

    while idx < chars.len() {
        let width = if is_first { widths[0] } else { widths[1] };
        if width == 0 {
            break;
        }
        let remaining = chars.len() - idx;
        let take = remaining.min(width);
        let mut end = idx + take;
        if end < chars.len() {
            let slice = &chars[idx..end];
            if let Some(last_space) = slice.iter().rposition(|c| c.is_whitespace()) {
                if last_space > 0 {
                    end = idx + last_space;
                }
            }
        }
        if end == idx {
            end = (idx + take).min(chars.len());
        }
        let segment = chars[idx..end].iter().collect::<String>().trim().to_string();
        lines.push(segment);
        idx = end;
        while idx < chars.len() && chars[idx].is_whitespace() {
            idx += 1;
        }
        is_first = false;
    }

    lines
}

fn iteration_adequacy(iteration: &RunIteration) -> Option<&AdequacyAssessment> {
    iteration.step_records.iter().rev().find_map(|record| match record {
        StepRecord::Finished(finished)
            if matches!(
                finished.step,
                StepKind::InformationAdequacyInitial
                    | StepKind::InformationAdequacySupportedObservation
                    | StepKind::InformationAdequacyUnsupportedObservation
            ) =>
        {
            match &finished.result {
                Ok(StepResultEnvelope::InformationAdequacy(assessment)) => Some(assessment),
                _ => None,
            }
        }
        _ => None,
    })
}

fn iteration_observation_extraction(
    iteration: &RunIteration,
) -> Option<&ObservationExtractionOutput> {
    iteration.step_records.iter().rev().find_map(|record| match record {
        StepRecord::Finished(finished) if finished.step == StepKind::ObservationExtraction => {
            match &finished.result {
                Ok(StepResultEnvelope::ObservationExtraction(output)) => Some(output),
                _ => None,
            }
        }
        _ => None,
    })
}

fn iteration_card_hydration(iteration: &RunIteration) -> Option<&CardHydrationOutput> {
    iteration.step_records.iter().rev().find_map(|record| match record {
        StepRecord::Finished(finished) if finished.step == StepKind::CardHydration => {
            match &finished.result {
                Ok(StepResultEnvelope::CardHydration(output)) => Some(output),
                _ => None,
            }
        }
        _ => None,
    })
}

fn summarize_query(input: &str, max_chars: usize) -> String {
    let trimmed = input.trim();
    if trimmed.chars().count() <= max_chars {
        trimmed.to_string()
    } else {
        let head: String = trimmed.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{head}...")
    }
}

fn wrap_with_ellipsis(
    text: &str,
    total_width: usize,
    first_prefix: &str,
    continuation_indent_width: usize,
    max_lines: usize,
) -> Vec<String> {
    let clean = text.trim();
    if clean.is_empty() || max_lines == 0 {
        return vec![String::new()];
    }

    let first_line_width = total_width.saturating_sub(first_prefix.chars().count());
    let continuation_width = total_width.saturating_sub(continuation_indent_width);
    let widths = std::iter::once(first_line_width)
        .chain(std::iter::repeat(continuation_width))
        .take(max_lines)
        .collect::<Vec<_>>();

    let chars: Vec<char> = clean.chars().collect();
    let mut idx = 0usize;
    let mut lines = Vec::new();

    for (line_index, width) in widths.iter().enumerate() {
        if idx >= chars.len() {
            break;
        }
        if *width == 0 {
            lines.push(String::new());
            continue;
        }

        let remaining = chars.len() - idx;
        let take = remaining.min(*width);
        let mut end = idx + take;
        let has_more = end < chars.len();

        if has_more {
            let slice = &chars[idx..end];
            if let Some(last_space_offset) = slice.iter().rposition(|c| c.is_whitespace()) {
                if last_space_offset > 0 {
                    end = idx + last_space_offset;
                }
            }
        }

        if end == idx {
            end = (idx + take).min(chars.len());
        }

        let mut segment: String = chars[idx..end].iter().collect();
        segment = segment.trim().to_string();
        idx = end;
        while idx < chars.len() && chars[idx].is_whitespace() {
            idx += 1;
        }

        let last_line = line_index + 1 == max_lines;
        if last_line && idx < chars.len() {
            let visible_width = width.saturating_sub(3);
            let mut visible: String = segment.chars().take(visible_width).collect();
            visible = visible.trim_end().to_string();
            if visible.is_empty() {
                visible = chars[end.saturating_sub((*width).min(3))..end]
                    .iter()
                    .collect();
            }
            segment = format!("{visible}...");
            idx = chars.len();
        }

        lines.push(segment);
    }

    if lines.is_empty() {
        vec![summarize_query(clean, total_width.max(3))]
    } else {
        lines
    }
}

fn run_status_short(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Active => "A",
        RunStatus::WaitingForUser => "W",
        RunStatus::Error => "E",
        RunStatus::Archived => "R",
    }
}

fn run_status_style(status: RunStatus) -> Style {
    match status {
        RunStatus::Active => Style::default().fg(Color::Cyan),
        RunStatus::WaitingForUser => Style::default().fg(Color::Yellow),
        RunStatus::Error => Style::default().fg(Color::Red),
        RunStatus::Archived => Style::default().fg(Color::Green),
    }
}

fn iteration_status_style(status: RunIterationStatus) -> Style {
    match status {
        RunIterationStatus::Active => Style::default().fg(Color::Cyan),
        RunIterationStatus::FinishedWithSuccess => Style::default().fg(Color::Green),
        RunIterationStatus::FinishedWithError => Style::default().fg(Color::Red),
        RunIterationStatus::FinishedWithWaitInput => Style::default().fg(Color::Yellow),
    }
}

fn hypothesis_status_label(status: &HypothesisStatus) -> &'static str {
    match status {
        HypothesisStatus::Active => "active",
        HypothesisStatus::Weakened => "weakened",
        HypothesisStatus::Rejected(_) => "rejected",
    }
}

fn hypothesis_source_label(source: HypothesisEvidenceSource) -> &'static str {
    match source {
        HypothesisEvidenceSource::PrimaryIncident => "Primary incident",
        HypothesisEvidenceSource::AlternativeContext => "Alternative incident",
        HypothesisEvidenceSource::TheoryMechanism => "Theory/mechanism",
    }
}

fn confidence_with_indicator(
    hypothesis: &distributed_diagnostics::shared_types::Hypothesis,
    previous_hypothesis: Option<&distributed_diagnostics::shared_types::Hypothesis>,
) -> String {
    let trend = previous_hypothesis.and_then(|previous| {
        if previous.status == hypothesis.status {
            None
        } else {
            Some(match hypothesis.status {
                HypothesisStatus::Active => "↑",
                HypothesisStatus::Weakened | HypothesisStatus::Rejected(_) => "↓",
            })
        }
    });

    match trend {
        Some(trend) => format!("{:?} {trend}", hypothesis.confidence),
        None => format!("{:?}", hypothesis.confidence),
    }
}

fn outcome_status_message(outcome: &RunOutcome) -> String {
    match outcome {
        RunOutcome::Finished { result, .. } => format!(
            "Run finished. Current problem understanding: {}",
            summarize_query(&result.response.problem_understanding, 72)
        ),
        RunOutcome::WaitingForUser {
            follow_up_questions, ..
        } => {
            if follow_up_questions.is_empty() {
                "Run is waiting for more user input.".to_string()
            } else {
                format!(
                    "Run is waiting for input: {}",
                    summarize_query(&follow_up_questions.join(" | "), 72)
                )
            }
        }
        RunOutcome::Failed { error, .. } => format!("Run failed: {error}"),
    }
}

fn panel_border_style(is_focused: bool) -> Style {
    if is_focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    }
}

fn resolve_ingest_config(
    runtime_config: &PathBuf,
    ingest_config: Option<PathBuf>,
) -> Result<PathBuf, Box<dyn Error>> {
    if let Some(path) = ingest_config {
        if same_path(runtime_config, &path) {
            return infer_sibling_ingest_config(runtime_config);
        }
        return Ok(path);
    }

    infer_sibling_ingest_config(runtime_config)
}

fn infer_sibling_ingest_config(runtime_config: &PathBuf) -> Result<PathBuf, Box<dyn Error>> {
    let Some(parent) = runtime_config.parent() else {
        return Err(
            "Could not infer ingest config path. Pass --ingest-config explicitly.".into(),
        );
    };

    let inferred = parent.join("ingest.toml");
    if inferred.exists() {
        Ok(inferred)
    } else {
        Err(format!(
            "Could not find inferred ingest config at {}. Pass --ingest-config explicitly.",
            inferred.display()
        )
        .into())
    }
}

fn same_path(left: &PathBuf, right: &PathBuf) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}
