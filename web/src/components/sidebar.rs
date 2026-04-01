//! Sidebar Component

use crate::components::Session;
use leptos::prelude::*;

#[component]
pub fn Sidebar(
    sessions: ReadSignal<Vec<Session>>,
    current_session_id: Option<String>,
    on_select: Callback<Session>,
) -> impl IntoView {
    let sessions_list = move || sessions.get();

    view! {
        <aside class="w-64 h-full bg-[#1a1a1a] border-r border-[#2d2d2d] flex flex-col">
            <div class="p-4">
                <h2 class="text-sm font-semibold text-gray-400 uppercase">Sessions</h2>
            </div>

            <div class="flex-1 overflow-y-auto">
                <ul class="space-y-1 px-2">
                    {move || {
                        sessions_list()
                            .into_iter()
                            .map(|session| {
                                let is_selected = current_session_id
                                    .as_ref()
                                    .map(|id| id == &session.id)
                                    .unwrap_or(false);
                                let on_select = on_select.clone();
                                let session_clone = session.clone();

                                view! {
                                    <li>
                                        <button
                                            class={format!(
                                                "w-full px-3 py-2 rounded text-sm truncate transition-colors text-left {}",
                                                if is_selected {
                                                    "bg-blue-600 text-white"
                                                } else {
                                                    "text-gray-300 hover:bg-[#2d2d2d]"
                                                }
                                            )}
                                            on:click={move |_| on_select.run(session_clone.clone())}
                                        >
                                            {session.display_title()}
                                        </button>
                                    </li>
                                }
                            })
                            .collect::<Vec<_>>()
                    }}
                </ul>
            </div>
        </aside>
    }
}
