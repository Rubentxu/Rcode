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

use rcode_core::{Message, Part, Session, SessionStatus};
use rcode_event::EventBus;
use rcode_session::SessionService;

use crate::app::{AppMode, OpencodeTui};
use crate::events::{parse_event, InputEvent};
use crate::views::{ChatView, InputView, SidebarView};

/// Run the TUI application
pub async fn run(
    session_service: Arc<SessionService>,
    event_bus: Arc<EventBus>,
) -> anyhow::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // State
    let mut app = OpencodeTui::new();
    let mut sidebar = SidebarView::new();
    let mut chat = ChatView::new();
    let mut input = InputView::new();

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
                        // Map event to input event and send
                        let _ = tx_clone.send(InputEvent::Tick).await;
                        let _ = event;
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
        &session_service,
        &event_bus,
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
    app: &mut OpencodeTui,
    sidebar: &mut SidebarView,
    chat: &mut ChatView,
    input: &mut InputView,
    session_service: &Arc<SessionService>,
    event_bus: &Arc<EventBus>,
    _tx: mpsc::Sender<InputEvent>,
    rx: &mut mpsc::Receiver<InputEvent>,
) -> anyhow::Result<()> {
    loop {
        // Draw
        terminal.draw(|f| {
            let size = f.area();
            let (sidebar_area, chat_area) = split_layout(size);

            sidebar.render(app, sidebar_area, f.buffer_mut());
            chat.render(app, chat_area, f.buffer_mut());
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
                        session_service,
                        event_bus,
                    )
                    .await?
                    {
                        return Ok(());
                    }
                }
            }
            // Async tick/event bus events
            Some(_async_event) = rx.recv() => {
                // Refresh messages if in chat mode
                if app.mode == AppMode::Chat {
                    if let Some(session_id) = &app.current_session {
                        app.update_messages(session_service.get_messages(&session_id.0));
                    }
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

async fn handle_input_event(
    event: InputEvent,
    app: &mut OpencodeTui,
    sidebar: &mut SidebarView,
    chat: &mut ChatView,
    input: &mut InputView,
    session_service: &Arc<SessionService>,
    _event_bus: &Arc<EventBus>,
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
                let session = Session::new(
                    std::path::PathBuf::from("."),
                    "default".to_string(),
                    "claude-sonnet-4-5".to_string(),
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

            // Mode-specific handling
            match app.mode {
                AppMode::SessionList | AppMode::Chat => {
                    handle_chat_mode_key(key, modifiers, app, sidebar, chat, input, session_service)
                        .await
                }
                AppMode::Settings => {
                    // TODO: Settings mode handling
                    Ok(false)
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
    }
}

async fn handle_chat_mode_key(
    key: KeyCode,
    modifiers: KeyModifiers,
    app: &mut OpencodeTui,
    sidebar: &mut SidebarView,
    chat: &mut ChatView,
    input: &mut InputView,
    session_service: &Arc<SessionService>,
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
                    let message = Message::user(
                        session_id_str.clone(),
                        vec![Part::Text {
                            content: text,
                        }],
                    );
                    session_service.add_message(&session_id_str, message);
                    app.update_messages(session_service.get_messages(&session_id_str));
                    app.is_running = true;
                    chat.reset_scroll();

                    // Update session status to running
                    session_service.update_status(&session_id.0, SessionStatus::Running);
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
