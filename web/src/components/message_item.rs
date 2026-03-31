//! MessageItem Component

use crate::components::Message;
use leptos::prelude::*;

#[component]
pub fn MessageItem(message: Message) -> impl IntoView {
    view! {
        <div class="mb-4 p-3 bg-[#252525] rounded-lg">
            <p class="text-sm text-gray-100">{message.id}</p>
        </div>
    }
}
