//! Routes Module

pub mod home;
pub mod sessions;
pub mod chat;
pub mod settings;

use leptos::prelude::*;

/// 404 Not Found page
#[component]
pub fn NotFound() -> impl IntoView {
    view! {
        <div class="flex items-center justify-center h-full bg-[#0f0f0f]">
            <div class="text-center">
                <h1 class="text-6xl font-bold text-gray-600">404</h1>
                <p class="text-xl text-gray-400 mt-4">Page not found</p>
            </div>
        </div>
    }
}
