//! MessageItem Component

use crate::components::{Message, Role};
use leptos::prelude::*;

#[component]
pub fn MessageItem(message: Message) -> impl IntoView {
    let role_color = match message.role {
        Role::User => "text-blue-400",
        Role::Assistant => "text-green-400",
        Role::System => "text-yellow-400",
    };

    let role_label = match message.role {
        Role::User => "User",
        Role::Assistant => "Assistant",
        Role::System => "System",
    };

    let text_content = message.text_content();

    view! {
        <div class="mb-4 p-4 bg-[#1a1a1a] rounded-lg border border-[#2d2d2d]">
            <div class="flex items-center gap-2 mb-2">
                <span class={format!("text-sm font-medium {}", role_color)}>
                    {role_label}
                </span>
                <span class="text-xs text-gray-500">
                    {message.created_at.format("%H:%M:%S").to_string()}
                </span>
            </div>
            <div class="text-gray-200 whitespace-pre-wrap">
                {text_content}
            </div>
        </div>
    }
}
