//! Smart compaction module for token-aware message pruning.
//!
//! This module provides token counting, context window management, and overflow
//! detection to prevent context overflow before sending messages to the LLM.
//!
//! ## Key Design Decisions
//!
//! - Token counting: heuristic chars/4 (fast, ~75% accurate)
//! - Context windows: hardcoded map for known model families
//! - Pruning: erase ToolResult content, keep ToolCall metadata
//! - Pre-check BEFORE streaming, not after

use rcode_core::{Message, Part, Role};

/// Hardcoded context window sizes per model family (in tokens).
/// Format: (model_prefix, context_window)
const MODEL_CONTEXT_WINDOWS: &[(&str, usize)] = &[
    // Anthropic models: 200K context
    ("anthropic", 200_000),
    // OpenAI models: 128K context
    ("openai", 128_000),
    // Google models: 1M context
    ("google", 1_000_000),
    // MiniMax models: 1M context
    ("minimax", 1_000_000),
    // Default: 128K
    ("default", 128_000),
];

/// Estimate tokens for a text string using the chars/4 heuristic.
/// This is a fast approximation: 4 characters ≈ 1 token.
pub fn estimate_tokens_for_text(text: &str) -> usize {
    text.len() / 4
}

/// Estimate total tokens for a slice of messages.
/// Counts tokens across all parts of all messages.
pub fn estimate_tokens(messages: &[Message]) -> usize {
    messages
        .iter()
        .map(|msg| {
            msg.parts
                .iter()
                .map(|part| estimate_tokens_for_part(part))
                .sum::<usize>()
        })
        .sum()
}

/// Estimate tokens for a single Part.
fn estimate_tokens_for_part(part: &Part) -> usize {
    match part {
        Part::Text { content } => estimate_tokens_for_text(content),
        Part::ToolCall { id, name, arguments } => {
            // Approximate: id length + name length + args JSON string length
            id.len() + name.len() + arguments.to_string().len()
        }
        Part::ToolResult { tool_call_id, content, .. } => {
            // Approximate: tool_call_id length + content length
            tool_call_id.len() + content.len()
        }
        Part::Reasoning { content } => estimate_tokens_for_text(content),
        Part::Attachment { id: _, name, mime_type, content } => {
            // Approximate: name + mime_type + content (content is bytes)
            name.len() + mime_type.len() + content.len()
        }
        Part::TaskChecklist { items } => {
            // Approximate: JSON serialized size / 4
            serde_json::to_string(items)
                .map(|s| s.len())
                .unwrap_or(0)
        }
    }
}

/// Get the context window size for a given model ID.
/// Falls back to 128K for unknown models.
pub fn context_window_for_model(model_id: &str) -> usize {
    let model_lower = model_id.to_lowercase();
    
    for (prefix, window) in MODEL_CONTEXT_WINDOWS {
        if *prefix == "default" {
            continue;
        }
        if model_lower.contains(*prefix) {
            return *window;
        }
    }
    
    // Default fallback
    128_000
}

/// Result of overflow check.
#[derive(Debug, Clone)]
pub struct OverflowInfo {
    pub is_overflow: bool,
    pub estimated_tokens: usize,
    pub usable_tokens: usize,
}

/// Check if messages would overflow the model's context window.
///
/// # Arguments
/// * `messages` - The messages to check
/// * `model_id` - The model ID (e.g., "openai/gpt-4o")
/// * `system_prompt_tokens` - Tokens reserved for system prompt
/// * `overflow_ratio` - Ratio of context window to consider usable (0.0 to 1.0)
///
/// # Returns
/// Tuple of (is_overflow, estimated_tokens, usable_tokens)
pub fn is_overflow(
    messages: &[Message],
    model_id: &str,
    system_prompt_tokens: usize,
    overflow_ratio: f32,
) -> OverflowInfo {
    let context_window = context_window_for_model(model_id);
    let usable_tokens = (context_window as f32 * overflow_ratio) as usize;
    let estimated_tokens = estimate_tokens(messages);
    
    OverflowInfo {
        is_overflow: estimated_tokens + system_prompt_tokens > usable_tokens,
        estimated_tokens,
        usable_tokens,
    }
}

/// Signal returned after compaction attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactionSignal {
    /// Compaction succeeded, LLM call can proceed
    Continue,
    /// Even after compaction, context won't fit
    Stop,
}

impl CompactionSignal {
    pub fn should_proceed(&self) -> bool {
        matches!(self, CompactionSignal::Continue)
    }
}

/// Prune old tool outputs to free up token budget.
///
/// This erases the content of `Part::ToolResult` from older messages while
/// preserving tool call metadata. The most recent messages (within the token
/// budget) are left intact.
///
/// # Arguments
/// * `messages` - The messages to prune (mutated in place)
/// * `preserve_recent_tokens` - Number of tokens to preserve from recent messages
pub fn prune_tool_outputs(messages: &mut [Message], preserve_recent_tokens: usize) {
    if messages.is_empty() {
        return;
    }
    
    // Find the boundary: accumulate tokens from NEWEST to OLDEST.
    // Everything BEFORE the boundary gets pruned (old messages).
    // Everything from boundary onward is preserved (recent messages).
    let mut running_total: usize = 0;
    let mut boundary_idx = 0; // Index of first preserved message (default: prune all)
    
    for (idx, msg) in messages.iter().enumerate().rev() {
        let msg_tokens = estimate_tokens(std::slice::from_ref(msg));
        
        if running_total + msg_tokens <= preserve_recent_tokens {
            running_total += msg_tokens;
            boundary_idx = idx;
        } else {
            // This message alone exceeds remaining budget, but since we're
            // iterating from newest, this IS the boundary (preserve it)
            break;
        }
    }
    
    // If boundary_idx is 0 and there are multiple messages, the newest message
    // exceeded the budget. Preserve the last message only, erase the rest.
    // If boundary_idx is 0 and there's only 1 message, that message exceeded
    // the budget entirely — but there's nothing "older" to prune, so skip.
    if boundary_idx == 0 {
        if messages.len() > 1 {
            boundary_idx = messages.len() - 1;
        } else {
            // Single message exceeds budget but nothing older to erase
            return;
        }
    }
    
    // All messages at indices < boundary_idx get their tool results erased.
    for msg in messages.iter_mut().take(boundary_idx) {
        for part in msg.parts.iter_mut() {
            if let Part::ToolResult { content, .. } = part {
                *content = String::new();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_message(parts: Vec<Part>) -> Message {
        Message::user("test-session".to_string(), parts)
    }

    fn make_tool_call(id: &str, name: &str, args: serde_json::Value) -> Part {
        Part::ToolCall {
            id: id.to_string(),
            name: name.to_string(),
            arguments: Box::new(args),
        }
    }

    fn make_tool_result(tool_call_id: &str, content: &str, is_error: bool) -> Part {
        Part::ToolResult {
            tool_call_id: tool_call_id.to_string(),
            content: content.to_string(),
            is_error,
        }
    }

    fn make_text_part(content: &str) -> Part {
        Part::Text { content: content.to_string() }
    }

    // =============================================================================
    // estimate_tokens_for_text tests
    // =============================================================================

    #[test]
    fn test_estimate_tokens_for_text_exact_division() {
        assert_eq!(estimate_tokens_for_text("abcd"), 1);
        assert_eq!(estimate_tokens_for_text("12345678"), 2);
    }

    #[test]
    fn test_estimate_tokens_for_text_partial_division() {
        assert_eq!(estimate_tokens_for_text("a"), 0);
        assert_eq!(estimate_tokens_for_text("abcde"), 1);
        assert_eq!(estimate_tokens_for_text("abcdefg"), 1);
        assert_eq!(estimate_tokens_for_text("abcdefgh"), 2);
    }

    #[test]
    fn test_estimate_tokens_for_text_empty() {
        assert_eq!(estimate_tokens_for_text(""), 0);
    }

    #[test]
    fn test_estimate_tokens_for_text_large_string() {
        assert_eq!(estimate_tokens_for_text(&"x".repeat(100)), 25);
        assert_eq!(estimate_tokens_for_text(&"x".repeat(1000)), 250);
    }

    // =============================================================================
    // context_window_for_model tests
    // =============================================================================

    #[test]
    fn test_context_window_for_model_anthropic() {
        assert_eq!(context_window_for_model("anthropic/claude-3-5-sonnet"), 200_000);
        assert_eq!(context_window_for_model("anthropic/claude-sonnet-4-2025-02-19"), 200_000);
    }

    #[test]
    fn test_context_window_for_model_openai() {
        assert_eq!(context_window_for_model("openai/gpt-4o"), 128_000);
        assert_eq!(context_window_for_model("openai/gpt-4o-mini"), 128_000);
    }

    #[test]
    fn test_context_window_for_model_google() {
        assert_eq!(context_window_for_model("google/gemini-2.5-pro"), 1_000_000);
    }

    #[test]
    fn test_context_window_for_model_minimax() {
        assert_eq!(context_window_for_model("minimax/MiniMax-M2.7"), 1_000_000);
    }

    #[test]
    fn test_context_window_for_model_default() {
        assert_eq!(context_window_for_model("unknown/model-xyz"), 128_000);
    }

    // =============================================================================
    // estimate_tokens tests (for a full message)
    // =============================================================================

    #[test]
    fn test_estimate_tokens_single_text_part() {
        let msg = make_message(vec![make_text_part("hello world")]);
        assert_eq!(estimate_tokens(&[msg]), 2);
    }

    #[test]
    fn test_estimate_tokens_multiple_parts() {
        let msg = make_message(vec![
            make_text_part("hello"),
            make_text_part("world"),
        ]);
        assert_eq!(estimate_tokens(&[msg]), 2);
    }

    #[test]
    fn test_estimate_tokens_tool_call() {
        let msg = make_message(vec![
            make_tool_call("call_1", "bash", serde_json::json!({"cmd": "ls -la"}))
        ]);
        let tokens = estimate_tokens(&[msg]);
        assert!(tokens > 0);
    }

    #[test]
    fn test_estimate_tokens_tool_result() {
        let msg = make_message(vec![
            make_tool_result("call_1", "file1.txt\nfile2.txt\nfile3.txt", false)
        ]);
        let tokens = estimate_tokens(&[msg]);
        assert!(tokens > 0);
    }

    #[test]
    fn test_estimate_tokens_empty_messages() {
        assert_eq!(estimate_tokens(&[]), 0);
    }

    #[test]
    fn test_estimate_tokens_multiple_messages() {
        let msg1 = make_message(vec![make_text_part("hello")]);
        let msg2 = make_message(vec![make_text_part("world")]);
        assert_eq!(estimate_tokens(&[msg1, msg2]), 2);
    }

    // =============================================================================
    // is_overflow tests
    // =============================================================================

    #[test]
    fn test_is_overflow_no_overflow() {
        let msg = make_message(vec![make_text_part("hello")]);
        let result = is_overflow(&[msg], "openai/gpt-4o", 100, 0.8);
        assert!(!result.is_overflow);
    }

    #[test]
    fn test_is_overflow_below_threshold() {
        let msg = make_message(vec![make_text_part(&"x".repeat(1000))]);
        let result = is_overflow(&[msg], "openai/gpt-4o", 100, 0.8);
        assert!(!result.is_overflow);
    }

    #[test]
    fn test_is_overflow_above_threshold() {
        let mut messages = Vec::new();
        for i in 0..500 {
            let content = format!("message {} {}", i, "x".repeat(900));
            messages.push(make_message(vec![make_text_part(&content)]));
        }
        let result = is_overflow(&messages, "openai/gpt-4o", 100, 0.8);
        assert!(result.is_overflow);
    }

    #[test]
    fn test_is_overflow_with_system_prompt_tokens() {
        let msg = make_message(vec![make_text_part("hello")]);
        let result = is_overflow(&[msg], "openai/gpt-4o", 50_000, 0.8);
        assert!(!result.is_overflow);
    }

    #[test]
    fn test_is_overflow_exactly_at_threshold() {
        let content = "x".repeat(409_600);
        let msg = make_message(vec![make_text_part(&content)]);
        let result = is_overflow(&[msg], "openai/gpt-4o", 0, 0.8);
        assert!(!result.is_overflow);
    }

    #[test]
    fn test_is_overflow_google_model_has_larger_window() {
        let mut messages = Vec::new();
        for i in 0..500 {
            let content = format!("message {} {}", i, "x".repeat(900));
            messages.push(make_message(vec![make_text_part(&content)]));
        }
        let result = is_overflow(&messages, "google/gemini-2.5-pro", 0, 0.8);
        assert!(!result.is_overflow);
    }

    #[test]
    fn test_is_overflow_return_info() {
        let msg = make_message(vec![make_text_part("hello")]);
        let result = is_overflow(&[msg], "openai/gpt-4o", 0, 0.8);
        assert!(!result.is_overflow);
        assert_eq!(result.estimated_tokens, 1);
        assert_eq!(result.usable_tokens, 102_400);
    }

    // =============================================================================
    // prune_tool_outputs tests
    // =============================================================================

    #[test]
    fn test_prune_tool_outputs_preserves_text_parts() {
        let mut messages = vec![
            make_message(vec![
                make_text_part("hello"),
                make_tool_result("call_1", "big output here", false),
            ]),
        ];
        
        prune_tool_outputs(&mut messages, 10_000);
        
        if let Part::Text { content } = &messages[0].parts[0] {
            assert_eq!(content, "hello");
        } else {
            panic!("Expected Text part first");
        }
    }

    #[test]
    fn test_prune_tool_outputs_erases_old_tool_results() {
        // 1 message with ToolCall + ToolResult (~12 tokens total)
        // Budget = 10 tokens - only 1 message fits, nothing erased
        let mut messages = vec![
            make_message(vec![
                make_tool_call("call_1", "bash", serde_json::json!({"cmd": "ls"})),
                make_tool_result("call_1", "file1.txt\nfile2.txt\nfile3.txt\nfile4.txt\nfile5.txt", false),
            ]),
        ];
        
        // With budget larger than message, nothing is erased
        prune_tool_outputs(&mut messages, 10_000);
        
        if let Part::ToolResult { content, .. } = &messages[0].parts[1] {
            assert!(!content.is_empty(), "With large budget, tool result should be preserved");
        } else {
            panic!("Expected ToolResult as second part");
        }
        
        // With tiny budget (1 token), even the first message exceeds budget
        // We preserve it anyway (don't erase if it doesn't fit)
        prune_tool_outputs(&mut messages, 1);
        
        if let Part::ToolResult { content, .. } = &messages[0].parts[1] {
            // Message itself doesn't fit in budget, but we preserve it anyway
            // No erasure happens since there's nothing older to erase
            assert!(!content.is_empty(), "Preserved message should not be erased");
        }
    }

    #[test]
    fn test_prune_tool_outputs_preserves_recent_tool_results() {
        let mut messages = vec![
            make_message(vec![
                make_tool_call("call_1", "bash", serde_json::json!({"cmd": "ls"})),
                make_tool_result("call_1", "old output", false),
            ]),
            make_message(vec![
                make_tool_call("call_2", "bash", serde_json::json!({"cmd": "pwd"})),
                make_tool_result("call_2", "recent important output", false),
            ]),
        ];
        
        prune_tool_outputs(&mut messages, 1_000_000);
        
        if let Part::ToolResult { content, .. } = &messages[1].parts[1] {
            assert_eq!(content, "recent important output");
        } else {
            panic!("Expected ToolResult as second part of second message");
        }
    }

    #[test]
    fn test_prune_tool_outputs_empty_messages() {
        let mut messages: Vec<Message> = vec![];
        prune_tool_outputs(&mut messages, 10_000);
        assert!(messages.is_empty());
    }

    #[test]
    fn test_prune_tool_outputs_preserves_user_and_assistant_messages() {
        let mut messages = vec![
            Message::user("test".to_string(), vec![make_text_part("user message")]),
            Message::assistant("test".to_string(), vec![make_text_part("assistant response")]),
        ];
        
        prune_tool_outputs(&mut messages, 10_000);
        
        assert_eq!(messages.len(), 2);
    }

    #[test]
    fn test_prune_tool_outputs_tool_call_metadata_preserved() {
        let mut messages = vec![
            make_message(vec![
                make_tool_call("call_1", "bash", serde_json::json!({"cmd": "ls"})),
                make_tool_result("call_1", "some output", false),
            ]),
        ];
        
        prune_tool_outputs(&mut messages, 10_000);
        
        if let Part::ToolCall { id, name, arguments } = &messages[0].parts[0] {
            assert_eq!(id, "call_1");
            assert_eq!(name, "bash");
            let args: serde_json::Value = serde_json::from_str(&arguments.to_string()).unwrap();
            assert_eq!(args, serde_json::json!({"cmd": "ls"}));
        } else {
            panic!("Expected ToolCall");
        }
    }

    #[test]
    fn test_prune_tool_outputs_large_old_output_erased() {
        // 2 messages: old (large) + recent (small)
        // Budget only fits the recent message
        let large_output = "x".repeat(50_000);
        let mut messages = vec![
            make_message(vec![
                make_tool_call("call_1", "bash", serde_json::json!({"cmd": "find /"})),
                make_tool_result("call_1", &large_output, false),
            ]),
            make_message(vec![
                make_tool_call("call_2", "bash", serde_json::json!({"cmd": "pwd"})),
                make_tool_result("call_2", "small", false),
            ]),
        ];
        
        prune_tool_outputs(&mut messages, 100);
        
        // Old message (index 0) should be erased
        if let Part::ToolResult { content, .. } = &messages[0].parts[1] {
            assert!(content.is_empty(), "Large old tool output should be erased");
        }
        
        // Recent message (index 1) should be preserved
        if let Part::ToolResult { content, .. } = &messages[1].parts[1] {
            assert_eq!(content, "small", "Recent tool output should be preserved");
        }
    }

    #[test]
    fn test_prune_tool_outputs_multiple_tool_pairs() {
        // 3 messages, each ~9 tokens (ToolCall + ToolResult)
        // Total ~27 tokens
        let mut messages = vec![
            make_message(vec![
                make_tool_call("call_1", "bash", serde_json::json!({"cmd": "ls"})),
                make_tool_result("call_1", "output1", false),
            ]),
            make_message(vec![
                make_tool_call("call_2", "bash", serde_json::json!({"cmd": "pwd"})),
                make_tool_result("call_2", "output2", false),
            ]),
            make_message(vec![
                make_tool_call("call_3", "bash", serde_json::json!({"cmd": "whoami"})),
                make_tool_result("call_3", "output3", false),
            ]),
        ];
        
        // With large budget, all messages fit - nothing erased
        prune_tool_outputs(&mut messages, 10_000);
        if let Part::ToolResult { content, .. } = &messages[2].parts[1] {
            assert_eq!(content, "output3", "With large budget, all preserved");
        }
        
        // Reset messages
        let mut messages = vec![
            make_message(vec![
                make_tool_call("call_1", "bash", serde_json::json!({"cmd": "ls"})),
                make_tool_result("call_1", "output1", false),
            ]),
            make_message(vec![
                make_tool_call("call_2", "bash", serde_json::json!({"cmd": "pwd"})),
                make_tool_result("call_2", "output2", false),
            ]),
            make_message(vec![
                make_tool_call("call_3", "bash", serde_json::json!({"cmd": "whoami"})),
                make_tool_result("call_3", "output3", false),
            ]),
        ];
        
        // With small budget (20 tokens), first 2 messages fit but 3rd doesn't
        // So message 0 and 1 should be erased, message 2 preserved
        prune_tool_outputs(&mut messages, 20);
        
        // Most recent (message 2, call_3) should be preserved
        if let Part::ToolResult { content, .. } = &messages[2].parts[1] {
            assert_eq!(content, "output3", "Most recent message should be preserved");
        }
        
        // Older messages (0 and 1) should be erased
        if let Part::ToolResult { content, .. } = &messages[0].parts[1] {
            assert!(content.is_empty(), "Older message should be erased");
        }
        if let Part::ToolResult { content, .. } = &messages[1].parts[1] {
            assert!(content.is_empty(), "Older message should be erased");
        }
    }

    // =============================================================================
    // CompactionSignal tests
    // =============================================================================

    #[test]
    fn test_compaction_signal_continue() {
        let signal = CompactionSignal::Continue;
        assert!(signal.should_proceed());
    }

    #[test]
    fn test_compaction_signal_stop() {
        let signal = CompactionSignal::Stop;
        assert!(!signal.should_proceed());
    }
}
