//! SessionView Component

use crate::components::chat_view::ChatView;
use crate::components::{Message, Session};
use leptos::prelude::*;

#[component]
pub fn SessionView(
    session: Session,
    messages: ReadSignal<Vec<Message>>,
    is_loading: ReadSignal<bool>,
    sse_status: ReadSignal<String>,
    on_submit: Callback<String>,
    on_abort: Callback<()>,
) -> impl IntoView {
    view! {
        <div class="flex flex-col h-full">
            <div class="h-14 border-b border-[#2d2d2d] flex items-center px-4 bg-[#1a1a1a]">
                <h1 class="text-lg font-semibold truncate">{session.display_title()}</h1>
                <span class={format!(
                    "ml-3 px-2 py-0.5 rounded text-xs {}",
                    match session.status {
                        crate::components::SessionStatus::Idle => "bg-gray-600 text-gray-300",
                        crate::components::SessionStatus::Running => "bg-blue-600 text-white",
                        crate::components::SessionStatus::Completed => "bg-green-600 text-white",
                        crate::components::SessionStatus::Aborted => "bg-red-600 text-white",
                    }
                )}>
                    {format!("{:?}", session.status)}
                </span>
                <span class="ml-auto flex items-center gap-2 text-xs text-gray-400">
                    <span class={format!(
                        "w-2 h-2 rounded-full {}",
                        match sse_status.get().as_str() {
                            "connected" => "bg-green-500",
                            "connecting" => "bg-yellow-500 animate-pulse",
                            _ => "bg-gray-500",
                        }
                    )}></span>
                    <span>{sse_status.get()}</span>
                </span>
            </div>
            <ChatView
                messages={messages}
                is_loading={is_loading}
                _session_id={session.id}
                on_submit={on_submit}
                on_abort={on_abort}
            />
        </div>
    }
}

#[component]
pub fn EmptySessionView(on_create_session: Callback<()>) -> impl IntoView {
    let handle_click = {
        let on_create_session = on_create_session.clone();
        move |_| {
            on_create_session.run(());
        }
    };

    view! {
        <div class="flex items-center justify-center h-full">
            <div class="text-center">
                <p class="text-gray-400 text-lg mb-2">"No session selected"</p>
                <p class="text-gray-500 text-sm mb-4">"Select a session from the sidebar or create a new one"</p>
                <button
                    class="bg-blue-600 hover:bg-blue-700 px-4 py-2 rounded font-medium transition-colors"
                    on:click={handle_click}
                >
                    "Create New Session"
                </button>
            </div>
        </div>
    }
}
