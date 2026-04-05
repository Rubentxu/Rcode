//! TUI run loop

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal, layout::Rect};
use std::io;
use std::sync::Arc;
use tokio::sync::mpsc;

use rcode_core::{Message, Part, Role, Session, SessionId, SessionStatus, AgentContext, RcodeConfig};
use rcode_event::Event;
use rcode_event::EventBus;
use rcode_session::SessionService;
use rcode_agent::{AgentExecutor, DefaultAgent};
use rcode_providers::ProviderFactory;
use rcode_tools::ToolRegistryService;

use crate::app::{AppMode, RcodeTui};
use crate::events::{parse_event, InputEvent};
use crate::views::{ChatView, InputView, ModelPickerView, SidebarView};

/// Run the TUI application
pub async fn run(
    session_service: Arc<SessionService>,
    event_bus: Arc<EventBus>,
    tools: Arc<ToolRegistryService>,
    config: RcodeConfig,
) -> anyhow::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // State
    let mut app = RcodeTui::new();
    let mut sidebar = SidebarView::new();
    let mut chat = ChatView::new();
    let mut input = InputView::new();
    let mut picker = ModelPickerView::new();

    // Load sessions from service
    app.sessions = session_service.list_sessions();

    // Event channel for async events
    let (tx, mut rx) = mpsc::channel::<InputEvent>(100);

    // Spawn async event listener
    let event_bus_clone = event_bus.clone();
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        let mut subscriber = event_bus_clone.subscribe();
        loop {
            tokio::select! {
                event = subscriber.recv() => {
                    if let Ok(event) = event {
                        // C11: Send actual bus event to be processed
                        let _ = tx_clone.send(InputEvent::BusEvent(event)).await;
                    }
                }
                _ = tokio::time::sleep(tokio::time::Duration::from_millis(50)) => {
                    let _ = tx_clone.send(InputEvent::Tick).await;
                }
            }
        }
    });

    // Main loop
    let res = run_loop(
        &mut terminal,
        &mut app,
        &mut sidebar,
        &mut chat,
        &mut input,
        &mut picker,
        &session_service,
        &event_bus,
        &tools,
        &config,
        tx,
        &mut rx,
    )
    .await;

    // Cleanup terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut RcodeTui,
    sidebar: &mut SidebarView,
    chat: &mut ChatView,
    input: &mut InputView,
    picker: &mut ModelPickerView,
    session_service: &Arc<SessionService>,
    event_bus: &Arc<EventBus>,
    tools: &Arc<ToolRegistryService>,
    config: &RcodeConfig,
    _tx: mpsc::Sender<InputEvent>,
    rx: &mut mpsc::Receiver<InputEvent>,
) -> anyhow::Result<()> {
    loop {
        // Draw
        terminal.draw(|f| {
            let size = f.area();
            match app.mode {
                AppMode::ModelPicker => {
                    picker.render(app, size, f.buffer_mut());
                }
                _ => {
                    let (sidebar_area, chat_area) = split_layout(size);
                    sidebar.render(app, sidebar_area, f.buffer_mut());
                    chat.render(app, chat_area, f.buffer_mut());
                }
            }
        })?;

            // Handle events
        tokio::select! {
            // Keyboard/mouse events from crossterm (sync call, run in blocking task)
            event = tokio::task::spawn_blocking(|| crossterm::event::read().ok()) => {
                let Ok(event) = event else { continue };
                let Some(event) = event else { continue };
                if let Some(input_event) = parse_event(event) {
                    if handle_input_event(
                        input_event,
                        app,
                        sidebar,
                        chat,
                        input,
                        picker,
                        session_service,
                        event_bus,
                        tools,
                        config,
                    )
                    .await?
                    {
                        return Ok(());
                    }
                }
            }
            // Async tick/event bus events
            Some(async_event) = rx.recv() => {
                // C11: Handle bus events for streaming and messages
                match async_event {
                    InputEvent::BusEvent(event) => {
                        handle_bus_event(&event, app, session_service);
                    }
                    InputEvent::Tick => {
                        // Refresh messages if in chat mode on tick
                        if app.mode == AppMode::Chat {
                            if let Some(session_id) = &app.current_session {
                                app.update_messages(session_service.get_messages(&session_id.0));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

fn split_layout(size: Rect) -> (Rect, Rect) {
    let sidebar_width = 30.min(size.width / 3);
    let sidebar_area = Rect::new(size.x, size.y, sidebar_width, size.height);
    let chat_area = Rect::new(
        size.x + sidebar_width,
        size.y,
        size.width - sidebar_width,
        size.height,
    );
    (sidebar_area, chat_area)
}

/// C11: Handle events from the event bus
fn handle_bus_event(
    event: &Event,
    app: &mut RcodeTui,
    session_service: &Arc<SessionService>,
) {
    match event {
        Event::StreamingProgress { session_id, accumulated_text, .. } => {
            // Append delta text to streaming display
            if let Some(current) = &app.current_session {
                if current.0 == *session_id {
                    app.append_streaming_delta(session_id, accumulated_text);
                }
            }
        }
        Event::MessageAdded { session_id, message_id: _ } => {
            // Note: message_id is available but we'd need to get the full message
            // For now, just refresh messages on any MessageAdded
            if let Some(current) = &app.current_session {
                if current.0 == *session_id {
                    app.update_messages(session_service.get_messages(session_id));
                    app.clear_streaming_delta(session_id);
                }
            }
        }
        Event::AgentFinished { session_id } => {
            // Set running to false when agent finishes
            if let Some(current) = &app.current_session {
                if current.0 == *session_id {
                    app.set_running(false);
                    // Refresh messages to show final state
                    app.update_messages(session_service.get_messages(session_id));
                }
            }
        }
        Event::SessionUpdated { session_id } => {
            // W8: Refresh session list when session is updated
            if let Some(current) = &app.current_session {
                if current.0 == *session_id {
                    // Reload session from service
                    if let Some(updated_session) = session_service.get(&SessionId(session_id.clone())) {
                        if let Some(pos) = app.sessions.iter().position(|s| s.id.0 == *session_id) {
                            app.sessions[pos] = updated_session;
                        }
                    }
                }
            }
        }
        _ => {
            // Other events - ignore in TUI
        }
    }
}

async fn handle_input_event(
    event: InputEvent,
    app: &mut RcodeTui,
    sidebar: &mut SidebarView,
    chat: &mut ChatView,
    input: &mut InputView,
    picker: &mut ModelPickerView,
    session_service: &Arc<SessionService>,
    event_bus: &Arc<EventBus>,
    tools: &Arc<ToolRegistryService>,
    config: &RcodeConfig,
) -> anyhow::Result<bool> {
    match event {
        InputEvent::Key(key_event) => {
            let key = key_event.code;
            let modifiers = key_event.modifiers;

            // Handle Ctrl+Q (quit)
            if modifiers == KeyModifiers::CONTROL && key == KeyCode::Char('q') {
                return Ok(true);
            }

            // Global Ctrl+N (new session)
            if modifiers == KeyModifiers::CONTROL && key == KeyCode::Char('n') {
                let default_model = config.model.clone().unwrap_or_else(|| "claude-sonnet-4-5".to_string());
                let session = Session::new(
                    std::path::PathBuf::from("."),
                    "default".to_string(),
                    default_model,
                );
                let session = session_service.create(session);
                app.create_session(session);
                chat.reset_scroll();
                return Ok(false);
            }

            // Global Ctrl+S (switch to selected)
            if modifiers == KeyModifiers::CONTROL && key == KeyCode::Char('s') {
                if let Some(session) = sidebar.selected_session(app) {
                    app.select_session(&session.id);
                    app.update_messages(session_service.get_messages(&session.id.0));
                    chat.reset_scroll();
                }
                return Ok(false);
            }

            // Global Ctrl+M or Ctrl+O (open model picker)
            if modifiers == KeyModifiers::CONTROL 
                && (key == KeyCode::Char('m') || key == KeyCode::Char('o')) {
                // Only open picker if we have a current session
                if app.current_session.is_some() {
                    // Load model list if not already loaded
                    if app.model_list.is_empty() {
                        let models = rcode_providers::ProviderFactory::list_models(config);
                        app.model_list = models
                            .into_iter()
                            .map(|m| (m.id, m.provider, m.enabled))
                            .collect();
                    }
                    // Initialize picker state
                    picker.init(app);
                    // Switch to picker mode
                    app.mode = AppMode::ModelPicker;
                }
                return Ok(false);
            }

            // Global Ctrl+Z (undo last exchange)
            if modifiers == KeyModifiers::CONTROL && key == KeyCode::Char('z') {
                if let Some(session_id) = &app.current_session {
                    if session_service.undo_last_exchange(&session_id.0).is_ok() {
                        app.update_messages(session_service.get_messages(&session_id.0));
                        chat.reset_scroll();
                    }
                }
                return Ok(false);
            }

            // Global Ctrl+Y (redo last exchange)
            if modifiers == KeyModifiers::CONTROL && key == KeyCode::Char('y') {
                if let Some(session_id) = &app.current_session {
                    if session_service.redo_last_exchange(&session_id.0).is_ok() {
                        app.update_messages(session_service.get_messages(&session_id.0));
                        chat.reset_scroll();
                    }
                }
                return Ok(false);
            }

            // Mode-specific handling
            match app.mode {
                AppMode::SessionList | AppMode::Chat => {
                    handle_chat_mode_key(key, modifiers, app, sidebar, chat, input, session_service, event_bus, tools, config)
                        .await
                }
                AppMode::Settings => {
                    // TODO: Settings mode handling
                    Ok(false)
                }
                AppMode::ModelPicker => {
                    handle_model_picker_key(key, modifiers, app, picker, session_service)
                        .await
                }
            }
        }
        InputEvent::Mouse(_mouse) => {
            // TODO: Mouse handling for clicking on sessions
            Ok(false)
        }
        InputEvent::Resize(_, _) => {
            // Terminal resize - ratatui handles this automatically
            Ok(false)
        }
        InputEvent::Tick => {
            // Periodic tick - refresh data
            Ok(false)
        }
        InputEvent::BusEvent(_) => {
            // Bus events are handled separately in the run_loop
            Ok(false)
        }
    }
}

async fn handle_chat_mode_key(
    key: KeyCode,
    modifiers: KeyModifiers,
    app: &mut RcodeTui,
    sidebar: &mut SidebarView,
    chat: &mut ChatView,
    input: &mut InputView,
    session_service: &Arc<SessionService>,
    event_bus: &Arc<EventBus>,
    tools: &Arc<ToolRegistryService>,
    config: &RcodeConfig,
) -> anyhow::Result<bool> {
    // Navigation keys when no input focused
    if app.input_buffer.is_empty() {
        match key {
            KeyCode::Up => {
                sidebar.previous(app);
                return Ok(false);
            }
            KeyCode::Down => {
                sidebar.next(app);
                return Ok(false);
            }
            KeyCode::PageUp => {
                chat.page_up(app);
                return Ok(false);
            }
            KeyCode::PageDown => {
                chat.page_down(app);
                return Ok(false);
            }
            _ => {}
        }
    }

    // Input handling
    match key {
        KeyCode::Enter => {
            if let Some(text) = input.take_message(app) {
                // Send message to session
                if let Some(session_id) = app.current_session.clone() {
                    let session_id_str = session_id.0.clone();
                    // D2: Track pre-existing message count for deduplication
                    let pre_existing_count = session_service.get_messages(&session_id_str).len();
                    let message = Message::user(
                        session_id_str.clone(),
                        vec![Part::Text {
                            content: text,
                        }],
                    );
                    session_service.add_message(&session_id_str, message.clone());
                    app.update_messages(session_service.get_messages(&session_id_str));
                    app.is_running = true;
                    chat.reset_scroll();

                    // Update session status to running
                    session_service.update_status(&session_id.0, SessionStatus::Running);

                    // Spawn a task to run the agent
                    let session_service_clone = Arc::clone(session_service);
                    let event_bus_clone = Arc::clone(event_bus);
                    let tools_clone = Arc::clone(tools);
                    let config_clone = config.clone();
                    
                    tokio::spawn(async move {
                        // Get session to build executor
                        let session = match session_service_clone.get(&session_id) {
                            Some(s) => s,
                            None => {
                                tracing::error!("Session not found: {}", session_id_str);
                                return;
                            }
                        };

                        // Build provider
                        let (provider, effective_model) = match ProviderFactory::build(&session.model_id, Some(&config_clone)) {
                            Ok((p, m)) => (p, m),
                            Err(e) => {
                                tracing::error!("Failed to build provider: {}", e);
                                let _ = session_service_clone.update_status(&session_id_str, SessionStatus::Aborted);
                                return;
                            }
                        };

                        // Create agent
                        let agent: Arc<dyn rcode_core::Agent> = Arc::new(DefaultAgent::new());

                        // Build executor
                        let executor = AgentExecutor::new(
                            agent,
                            provider,
                            tools_clone,
                        )
                        .with_event_bus(event_bus_clone.clone());

                        // Create agent context
                        let cwd = std::env::current_dir().unwrap_or_else(|_| session.project_path.clone());
                        let messages = session_service_clone.get_messages(&session_id_str);
                        
                        let mut ctx = AgentContext {
                            session_id: session_id_str.clone(),
                            project_path: session.project_path.clone(),
                            cwd,
                            user_id: None,
                            model_id: effective_model,
                            messages,
                        };

                        // Run the executor
                        let result = executor.run(&mut ctx).await;

                        // D2: Persist only NEW assistant messages (not previously persisted)
                        let new_messages = ctx.messages.iter().skip(pre_existing_count);
                        for msg in new_messages {
                            if msg.role == Role::Assistant {
                                session_service_clone.add_message(&session_id_str, msg.clone());
                            }
                        }

                        // Update session status based on result
                        match result {
                            Ok(_) => {
                                // G5: Set to Idle so session can accept new prompts
                                let _ = session_service_clone.update_status(&session_id_str, SessionStatus::Idle);
                            }
                            Err(e) => {
                                tracing::error!("Agent execution failed: {}", e);
                                let _ = session_service_clone.update_status(&session_id_str, SessionStatus::Aborted);
                            }
                        }

                        // Publish agent finished event
                        event_bus_clone.publish(rcode_event::Event::AgentFinished {
                            session_id: session_id_str,
                        });
                    });
                }
            }
        }
        KeyCode::Char(c) => {
            input.insert_char(c, app);
        }
        KeyCode::Backspace => {
            input.backspace(app);
        }
        KeyCode::Delete => {
            input.delete(app);
        }
        KeyCode::Left => {
            input.move_cursor_left();
        }
        KeyCode::Right => {
            input.move_cursor_right(app);
        }
        KeyCode::Home => {
            if modifiers == KeyModifiers::CONTROL {
                input.move_cursor_to_start();
            }
        }
        KeyCode::End => {
            if modifiers == KeyModifiers::CONTROL {
                input.move_cursor_to_end(app);
            }
        }
        KeyCode::Esc => {
            input.clear(app);
        }
        _ => {}
    }

    Ok(false)
}

/// Handle key events in model picker mode
async fn handle_model_picker_key(
    key: KeyCode,
    _modifiers: KeyModifiers,
    app: &mut RcodeTui,
    picker: &mut ModelPickerView,
    session_service: &Arc<SessionService>,
) -> anyhow::Result<bool> {
    match key {
        KeyCode::Esc => {
            // Cancel and return to chat mode
            app.mode = AppMode::Chat;
            picker.reset();
            return Ok(false);
        }
        KeyCode::Enter => {
            // Select current model
            if let Some((model_id, _, _)) = picker.selected_model(app) {
                if let Some(session_id) = &app.current_session {
                    // Update session model via service
                    let _ = session_service.update_model(&session_id.0, model_id.clone());
                    
                    // Reload session from SessionService to get fresh Arc<Session>
                    if let Some(updated_session) = session_service.get(&session_id) {
                        if let Some(pos) = app.sessions.iter().position(|s| s.id.0 == session_id.0) {
                            app.sessions[pos] = updated_session;
                        }
                    }
                }
            }
            app.mode = AppMode::Chat;
            picker.reset();
            return Ok(false);
        }
        KeyCode::Up => {
            picker.previous(app);
            return Ok(false);
        }
        KeyCode::Down => {
            picker.next(app);
            return Ok(false);
        }
        KeyCode::Left => {
            picker.previous_provider(app);
            return Ok(false);
        }
        KeyCode::Right => {
            picker.next_provider(app);
            return Ok(false);
        }
        _ => {}
    }
    Ok(false)
}
