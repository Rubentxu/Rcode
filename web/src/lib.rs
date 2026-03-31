//! OpenCode Web UI - Leptos Frontend

pub mod api;
pub mod components;
pub mod routes;

use leptos::prelude::*;

#[component]
pub fn App() -> impl IntoView {
    view! {
        <div class="flex flex-col h-full bg-[#0f0f0f] text-gray-100">
            <header class="h-14 bg-[#1a1a1a] border-b border-[#2d2d2d] flex items-center px-4">
                <h1 class="text-lg font-semibold">OpenCode</h1>
            </header>
            <main class="flex-1 p-8">
                <div class="max-w-2xl mx-auto">
                    <h2 class="text-2xl font-bold mb-4">Sessions</h2>
                    <p class="text-gray-400">Loading sessions...</p>
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
