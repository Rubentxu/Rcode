//! SessionView Component

use crate::components::{Message, Session};
use leptos::prelude::*;

#[component]
pub fn SessionView(
    session: Session,
    messages: ReadSignal<Vec<Message>>,
    is_loading: ReadSignal<bool>,
    on_submit: Callback<String>,
    on_abort: Option<Callback<()>>,
) -> impl IntoView {
    view! {
        <div class="flex flex-col h-full">
            <h1>{session.display_title()}</h1>
        </div>
    }
}

#[component]
pub fn EmptySessionView() -> impl IntoView {
    view! {
        <div class="flex items-center justify-center h-full">
            <p class="text-gray-400">No session selected</p>
        </div>
    }
}
