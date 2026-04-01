//! Header Component

use leptos::prelude::*;

#[component]
pub fn Header(
    #[prop(default = "OpenCode")] title: &'static str,
    _on_menu_click: Option<Callback<()>>,
) -> impl IntoView {
    view! {
        <header class="flex items-center justify-between h-14 px-4 bg-[#1a1a1a] border-b border-[#2d2d2d]">
            <h1 class="text-lg font-semibold text-gray-100">{title}</h1>
        </header>
    }
}
