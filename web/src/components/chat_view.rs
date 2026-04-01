//! ChatView Component

use crate::components::message_item::MessageItem;
use crate::components::Message;
use leptos::prelude::*;

#[component]
pub fn ChatView(
    messages: ReadSignal<Vec<Message>>,
    is_loading: ReadSignal<bool>,
    _session_id: String,
    on_submit: Callback<String>,
    on_abort: Callback<()>,
) -> impl IntoView {
    let (input_text, set_input_text) = signal(String::new());

    let submit = move |_| {
        let text = input_text.get();
        if !text.is_empty() {
            on_submit.run(text);
            set_input_text.set(String::new());
        }
    };

    let handle_abort = {
        let on_abort = on_abort.clone();
        move |_| {
            on_abort.run(());
        }
    };

    view! {
        <div class="flex flex-col h-full">
            <div class="flex-1 overflow-y-auto p-4">
                {move || messages.get().iter().map(|msg| {
                    view! {
                        <MessageItem message={msg.clone()} />
                    }
                }).collect::<Vec<_>>()}
            </div>

            <div class="flex items-center gap-2 px-4 py-2 border-t border-[#2d2d2d]">
                <Show when={move || is_loading.get()}>
                    <span class="text-gray-400 text-sm">"Processing..."</span>
                    <button
                        class="ml-auto text-sm text-red-400 hover:text-red-300"
                        on:click={handle_abort}
                    >
                        "Abort"
                    </button>
                </Show>
            </div>

            <div class="flex gap-2 p-4 border-t border-[#2d2d2d]">
                <textarea
                    class="flex-1 bg-[#252525] text-white rounded p-3 resize-none min-h-[60px]"
                    placeholder="Type your prompt..."
                    on:input={move |e| set_input_text.set(event_target_value(&e))}
                ></textarea>
                <button
                    class="bg-blue-600 hover:bg-blue-700 px-6 py-3 rounded font-medium transition-colors disabled:opacity-50"
                    disabled={move || is_loading.get()}
                    on:click={submit}
                >
                    "Send"
                </button>
            </div>
        </div>
    }
}
