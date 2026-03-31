//! ChatView Component

use crate::components::Message;
use leptos::prelude::*;

#[component]
pub fn ChatView(
    messages: ReadSignal<Vec<Message>>,
    on_submit: Callback<String>,
    on_abort: Option<Callback<()>>,
    is_loading: ReadSignal<bool>,
) -> impl IntoView {
    view! {
        <div class="flex flex-col h-full">
            <div class="flex-1 p-4">
                <p class="text-gray-400">Chat messages will appear here</p>
            </div>
        </div>
    }
}
