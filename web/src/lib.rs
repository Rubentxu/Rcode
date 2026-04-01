//! OpenCode Web UI - Leptos Frontend

pub mod api;
pub mod components;
pub mod routes;
pub mod sse;

use crate::api as api_module;
use crate::components::{Message, Session};
use crate::components::sidebar::Sidebar;
use crate::components::session_view::{EmptySessionView, SessionView};
use crate::sse::{spawn_sse_listener, SseState};
use leptos::callback::Callback;
use leptos::task::spawn;
use leptos::prelude::*;
use std::sync::Arc;

#[component]
pub fn App() -> impl IntoView {
    let (sessions, set_sessions) = signal(Vec::<Session>::new());
    let (current_session, set_current_session) = signal(None::<Session>);
    let (messages, set_messages) = signal(Vec::<Message>::new());
    let (is_loading, set_is_loading) = signal(false);
    let (sse_status, set_sse_status) = signal(String::from("disconnected"));
    
    let sse_state = Arc::new(SseState::new());

    let load_sessions = {
        let set_sessions = set_sessions.clone();
        move || {
            spawn(async move {
                match api_module::list_sessions().await {
                    Ok(list) => set_sessions.set(list),
                    Err(e) => tracing::error!("Failed to load sessions: {}", e),
                }
            });
        }
    };

    let load_messages = {
        let set_messages = set_messages.clone();
        move |session_id: String| {
            spawn(async move {
                match api_module::get_messages(&session_id, 0, 100).await {
                    Ok(paginated) => set_messages.set(paginated.messages),
                    Err(e) => tracing::error!("Failed to load messages: {}", e),
                }
            });
        }
    };

    let select_session = {
        let set_current_session = set_current_session.clone();
        let load_messages = load_messages.clone();
        let sse_state = sse_state.clone();
        let set_sse_status = set_sse_status.clone();
        Callback::new(move |session: Session| {
            set_current_session.set(Some(session.clone()));
            load_messages(session.id.clone());
            
            set_sse_status.set(String::from("connecting"));
            sse_state.subscribe(session.id.clone());
            
            spawn_sse_listener(session.id.clone(), {
                let set_messages = set_messages.clone();
                move |new_message| {
                    set_messages.update(|msgs| {
                        if !msgs.iter().any(|m| m.id == new_message.id) {
                            msgs.push(new_message);
                        }
                    });
                }
            });
            
            set_sse_status.set(String::from("connected"));
        })
    };

    let create_session = {
        let set_current_session = set_current_session.clone();
        let load_sessions = load_sessions.clone();
        let load_messages = load_messages.clone();
        Callback::new(move |_| {
            spawn(async move {
                match api_module::create_session("/tmp/test-project").await {
                    Ok(session) => {
                        set_current_session.set(Some(session.clone()));
                        load_sessions();
                        load_messages(session.id);
                    }
                    Err(e) => tracing::error!("Failed to create session: {}", e),
                }
            });
        })
    };

    let submit_prompt = {
        let current_session = current_session.clone();
        let set_is_loading = set_is_loading.clone();
        let load_messages = load_messages.clone();
        Callback::new(move |prompt: String| {
            if let Some(s) = current_session.get() {
                set_is_loading.set(true);
                let session_id = s.id.clone();
                let load_messages = load_messages.clone();
                spawn(async move {
                    match api_module::submit_prompt(&session_id, &prompt).await {
                        Ok(_response) => {
                            load_messages(session_id);
                        }
                        Err(e) => tracing::error!("Failed to submit prompt: {}", e),
                    }
                    set_is_loading.set(false);
                });
            }
        })
    };

    let abort_session = {
        let current_session = current_session.clone();
        let load_sessions = load_sessions.clone();
        Callback::new(move |_| {
            if let Some(s) = current_session.get() {
                let session_id = s.id.clone();
                spawn(async move {
                    match api_module::abort_session(&session_id).await {
                        Ok(_) => {
                            load_sessions();
                        }
                        Err(e) => tracing::error!("Failed to abort session: {}", e),
                    }
                });
            }
        })
    };

    Effect::new(move |_| {
        load_sessions();
    });

    view! {
        <div class="flex flex-col h-screen bg-[#0f0f0f] text-gray-100">
            <header class="h-14 bg-[#1a1a1a] border-b border-[#2d2d2d] flex items-center px-4">
                <h1 class="text-lg font-semibold">OpenCode</h1>
            </header>
            <main class="flex-1 flex overflow-hidden">
                <Sidebar
                    sessions={sessions}
                    current_session_id={current_session.get().map(|s| s.id)}
                    on_select={select_session}
                />
                <div class="flex-1 flex flex-col overflow-hidden">
                    <Show
                        when={move || current_session.get().is_some()}
                        fallback={move || view! { <EmptySessionView on_create_session={create_session} /> }}
                    >
                        <SessionView
                            session={current_session.get().unwrap()}
                            messages={messages}
                            is_loading={is_loading}
                            sse_status={sse_status}
                            on_submit={submit_prompt}
                            on_abort={abort_session}
                        />
                    </Show>
                </div>
            </main>
        </div>
    }
}

pub fn mount() {
    _ = console_log::init();
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}
