//! Sidebar Component

use crate::components::Session;
use leptos::prelude::*;

#[component]
pub fn Sidebar(
    sessions: ReadSignal<Vec<Session>>,
    current_session_id: ReadSignal<Option<String>>,
    on_select: Callback<String>,
    on_delete: Option<Callback<String>>,
    on_new: Option<Callback<()>>,
) -> impl IntoView {
    view! {
        <aside class="w-64 h-full bg-[#1a1a1a] border-r border-[#2d2d2d]">
            <div class="p-4">
                <h2 class="text-sm font-semibold text-gray-400 uppercase">Sessions</h2>
            </div>
        </aside>
    }
}
