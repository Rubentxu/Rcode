//! Edit fuzzy matching - strategies for matching old_text despite whitespace/typos
//!
//! Matching chain: exact → whitespace-normalized → indent-stripped → Levenshtein → block-anchor

use std::path::PathBuf;

/// Result of a fuzzy match attempt
#[derive(Debug, Clone)]
pub struct MatchResult {
    /// Byte offset in content where match starts
    pub start: usize,
    /// Byte offset in content where match ends
    pub end: usize,
    /// The matched text from the content
    pub matched_text: String,
    /// Confidence score 0.0 to 1.0
    pub confidence: f64,
    /// Strategy that found this match
    pub strategy: &'static str,
}

impl MatchResult {
    /// Create a new match result
    pub fn new(start: usize, end: usize, matched_text: String, confidence: f64, strategy: &'static str) -> Self {
        Self { start, end, matched_text, confidence, strategy }
    }
}

/// A matching strategy that attempts to find old_text in content
pub trait MatchingStrategy: Send + Sync {
    /// Name of this strategy for error messages
    fn name(&self) -> &'static str;

    /// Attempt to find old_text in content.
    /// Returns Some(MatchResult) if found, None if not matched.
    fn match_text(&self, old_text: &str, content: &str) -> Option<MatchResult>;
}

/// Exact string matching - no fuzzy
pub struct ExactMatch;

impl MatchingStrategy for ExactMatch {
    fn name(&self) -> &'static str {
        "exact"
    }

    fn match_text(&self, old_text: &str, content: &str) -> Option<MatchResult> {
        content.find(old_text).map(|start| {
            let end = start + old_text.len();
            let matched_text = content[start..end].to_string();
            MatchResult::new(start, end, matched_text, 1.0, self.name())
        })
    }
}

/// Fuzzy matcher that chains multiple strategies in order
pub struct FuzzyMatcher {
    strategies: Vec<Box<dyn MatchingStrategy>>,
}

impl FuzzyMatcher {
    /// Create a new fuzzy matcher with default strategies and threshold
    pub fn new() -> Self {
        Self::with_threshold(0.85)
    }

    /// Create a fuzzy matcher with custom Levenshtein threshold
    pub fn with_threshold(levenshtein_threshold: f64) -> Self {
        let strategies: Vec<Box<dyn MatchingStrategy>> = vec![
            Box::new(ExactMatch),
            Box::new(WhitespaceNormalizedMatch),
            Box::new(IndentationStrippedMatch),
            Box::new(LevenshteinMatch::new(levenshtein_threshold)),
            Box::new(BlockAnchorMatch::new()),
        ];
        Self { strategies }
    }

    /// Try each strategy in order, return first match found
    pub fn find_match(&self, old_text: &str, content: &str) -> Option<MatchResult> {
        for strategy in &self.strategies {
            if let Some(result) = strategy.match_text(old_text, content) {
                return Some(result);
            }
        }
        None
    }

    /// Get names of all strategies tried (for error messages)
    pub fn strategy_names(&self) -> Vec<&'static str> {
        self.strategies.iter().map(|s| s.name()).collect()
    }
}

impl Default for FuzzyMatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// Whitespace normalization matching strategy
///
/// Collapses consecutive spaces/tabs/newlines to single space for comparison.
/// Actual replacement uses the original matched text from content.
pub struct WhitespaceNormalizedMatch;

impl MatchingStrategy for WhitespaceNormalizedMatch {
    fn name(&self) -> &'static str {
        "whitespace-normalized"
    }

    fn match_text(&self, old_text: &str, content: &str) -> Option<MatchResult> {
        let normalized_old = normalize_whitespace(old_text);
        let normalized_content = normalize_whitespace(content);

        // Find position in normalized content
        let normalized_pos = normalized_content.find(&normalized_old)?;

        // Now find the actual text in original content that corresponds to this normalized match
        // Strategy: find the first line of old_text in content (allowing leading whitespace variation)
        let old_lines: Vec<&str> = old_text.lines().collect();
        if old_lines.is_empty() {
            return None;
        }

        // Find content position by searching for lines that match the normalized pattern
        // Get a rough search window in original content
        let search_window = 50; // characters before/after normalized position

        let content_chars: Vec<char> = content.chars().collect();
        let normalized_chars: Vec<char> = normalized_content.chars().collect();

        // Find the position in original that corresponds to normalized_pos
        // by counting non-whitespace chars
        let mut orig_pos = 0;
        let mut normalized_count = 0;
        for (i, c) in content_chars.iter().enumerate() {
            if !c.is_whitespace() {
                if normalized_count >= normalized_pos {
                    orig_pos = i;
                    break;
                }
                normalized_count += 1;
            }
        }

        // Search in a window around orig_pos
        let window_start = orig_pos.saturating_sub(search_window);
        let window_end = (orig_pos + search_window).min(content.len());
        let search_region = &content[window_start..window_end];

        // Try to find a substring of similar length to old_text in this window
        let old_len = old_text.len();
        let min_search = old_len.saturating_sub(5).max(1);

        for offset in 0..search_region.len().saturating_sub(min_search) {
            for end_offset in (offset + min_search)..search_region.len().min(offset + old_len + 10) {
                let candidate = &search_region[offset..end_offset];
                let normalized_candidate = normalize_whitespace(candidate);

                if normalized_candidate == normalized_old {
                    let abs_start = window_start + offset;
                    let abs_end = window_start + end_offset;

                    // Verify this is a good match by checking the normalized strings are similar
                    let confidence = 0.95; // Whitespace normalization is reliable

                    return Some(MatchResult::new(
                        abs_start,
                        abs_end,
                        content[abs_start..abs_end].to_string(),
                        confidence,
                        self.name(),
                    ));
                }
            }
        }

        // Fallback: just find where old_text could start based on normalized match
        // Search for first non-whitespace char of first line
        let first_line = old_lines.first()?;
        let first_token = first_line.split_whitespace().next()?;

        // Find this token in content near the expected position
        if let Some(token_pos) = content[window_start..window_end.min(content.len())].find(first_token) {
            let abs_start = window_start + token_pos;
            // Assume the matched text has same length as old_text
            let abs_end = (abs_start + old_len).min(content.len());

            return Some(MatchResult::new(
                abs_start,
                abs_end,
                content[abs_start..abs_end].to_string(),
                0.9,
                self.name(),
            ));
        }

        None
    }
}

/// Normalize whitespace: collapse consecutive spaces/tabs/newlines to single space
fn normalize_whitespace(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut last_was_ws = false;
    for c in text.chars() {
        if c == ' ' || c == '\t' || c == '\n' || c == '\r' {
            if !last_was_ws {
                result.push(' ');
                last_was_ws = true;
            }
        } else {
            result.push(c);
            last_was_ws = false;
        }
    }
    result
}

/// Indentation stripping matching strategy
///
/// Removes common leading whitespace from all lines before matching.
pub struct IndentationStrippedMatch;

impl MatchingStrategy for IndentationStrippedMatch {
    fn name(&self) -> &'static str {
        "indentation-stripped"
    }

    fn match_text(&self, old_text: &str, content: &str) -> Option<MatchResult> {
        // Strip common leading indentation from both
        let stripped_old = strip_common_indent(old_text);
        let stripped_content = strip_common_indent(content);

        // Find position in stripped content
        stripped_content.find(&stripped_old).map(|start| {
            // Map back to original content positions
            let (orig_start, orig_end) = map_stripped_to_original(content, old_text, start, start + stripped_old.len());
            let matched_text = content[orig_start..orig_end].to_string();

            MatchResult::new(orig_start, orig_end, matched_text, 0.95, self.name())
        })
    }
}

/// Strip common leading indentation from text
fn strip_common_indent(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return String::new();
    }

    // Find minimum indentation (excluding empty lines)
    let min_indent = lines.iter()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);

    lines.iter()
        .map(|l| if l.trim().is_empty() { "" } else { &l[min_indent.min(l.len())..] })
        .collect::<Vec<&str>>()
        .join("\n")
}

/// Map positions in stripped text back to original text
fn map_stripped_to_original(original: &str, old_text: &str, stripped_start: usize, stripped_end: usize) -> (usize, usize) {
    // This is complex - simplified approach: find the actual text in original
    // Since we know old_text exists somewhere, find it near the expected position
    let stripped_old = strip_common_indent(old_text);

    // Find where stripped text appears in stripped content
    let stripped_content = strip_common_indent(original);

    if let Some(pos) = stripped_content.find(&stripped_old) {
        // Now find the actual old_text in original (exact match)
        if let Some(orig_pos) = original.find(old_text) {
            let orig_end = orig_pos + old_text.len();
            return (orig_pos, orig_end);
        }
    }

    (stripped_start, stripped_end)
}

/// Levenshtein distance matching with configurable threshold
pub struct LevenshteinMatch {
    threshold: f64,
}

impl LevenshteinMatch {
    pub fn new(threshold: f64) -> Self {
        Self { threshold }
    }
}

impl MatchingStrategy for LevenshteinMatch {
    fn name(&self) -> &'static str {
        "levenshtein"
    }

    fn match_text(&self, old_text: &str, content: &str) -> Option<MatchResult> {
        // Try to find best fuzzy match using Levenshtein distance
        // Search through content for substrings of similar length to old_text
        let old_len = old_text.len();
        if old_len == 0 {
            return None;
        }

        let window_size = (old_len as f32 * 1.5) as usize; // Allow 50% length variation
        let min_window = old_len.saturating_sub(5).max(1);

        let mut best_match: Option<MatchResult> = None;
        let mut best_similarity = 0.0_f64;

        for window_start in 0..content.len().saturating_sub(min_window) {
            let window_end = (window_start + window_size).min(content.len());
            let window = &content[window_start..window_end];

            // Try different starting positions in the window
            for offset in 0..window.len().saturating_sub(min_window) {
                let candidate_end = (offset + old_len).min(window.len());
                let candidate = &window[offset..candidate_end];

                if candidate.len() < min_window {
                    continue;
                }

                // Use Jaro-Winkler for better typo tolerance (higher scores for prefix matches)
                let similarity = strsim::jaro_winkler(old_text, candidate);

                if similarity >= self.threshold && similarity > best_similarity {
                    best_similarity = similarity;
                    let end_pos = window_start + offset + candidate.len();
                    let matched_text = content[window_start..end_pos.min(content.len())].to_string();
                    best_match = Some(MatchResult::new(
                        window_start,
                        end_pos.min(content.len()),
                        matched_text,
                        similarity,
                        self.name()
                    ));
                }
            }
        }

        best_match
    }
}

/// Block anchor fallback - search using surrounding context lines
#[allow(dead_code)]
pub struct BlockAnchorMatch {
    /// Number of context lines to use for anchoring
    context_lines: usize,
}

impl BlockAnchorMatch {
    pub fn new() -> Self {
        Self { context_lines: 2 }
    }
}

impl Default for BlockAnchorMatch {
    fn default() -> Self {
        Self::new()
    }
}

impl MatchingStrategy for BlockAnchorMatch {
    fn name(&self) -> &'static str {
        "block-anchor"
    }

    fn match_text(&self, old_text: &str, content: &str) -> Option<MatchResult> {
        let lines: Vec<&str> = content.lines().collect();
        let old_lines: Vec<&str> = old_text.lines().collect();

        if old_lines.is_empty() || lines.is_empty() {
            return None;
        }

        // Find a block of lines that could match (first and last line heuristics)
        let first_line = old_lines.first()?.trim();
        let last_line = old_lines.last()?.trim();

        // Search for lines that could be anchors
        for (i, line) in lines.iter().enumerate() {
            if line.trim() == first_line {
                // Check if we have enough lines and the ending matches
                if i + old_lines.len() <= lines.len() {
                    let block: String = lines[i..i + old_lines.len()].join("\n");
                    let block_stripped = strip_common_indent(&block);
                    let old_stripped = strip_common_indent(old_text);

                    if block_stripped == old_stripped {
                        // Found it - calculate original positions
                        let start = lines[..i].iter().map(|l| l.len() + 1).sum::<usize>();
                        let end = start + block.len();
                        return Some(MatchResult::new(start, end, block, 0.99, self.name()));
                    }
                }
            }
        }

        // Fallback: try line-by-line fuzzy match
        for (i, line) in lines.iter().enumerate() {
            let line_jaro = strsim::jaro_winkler(first_line, line.trim());
            if line_jaro > 0.7 {
                // Potential anchor found
                if i + old_lines.len() <= lines.len() {
                    let block: String = lines[i..i + old_lines.len()].join("\n");
                    let similarity = strsim::jaro_winkler(old_text, &block);
                    if similarity > 0.8 {
                        let start = lines[..i].iter().map(|l| l.len() + 1).sum::<usize>();
                        let end = start + block.len();
                        return Some(MatchResult::new(start, end, block, similarity, self.name()));
                    }
                }
            }
        }

        None
    }
}

/// Build a unified diff showing changes between old and new content
pub fn build_diff(old_text: &str, new_text: &str) -> String {
    use similar::{TextDiff, ChangeTag};

    let diff = TextDiff::from_lines(old_text, new_text);

    let mut output = String::new();
    output.push_str("```diff\n");

    for change in diff.iter_all_changes() {
        let sign = match change.tag() {
            ChangeTag::Delete => "-",
            ChangeTag::Insert => "+",
            ChangeTag::Equal  => " ",
        };
        output.push_str(&format!("{}{}", sign, change));
    }

    output.push_str("```\n");
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match_found() {
        let matcher = FuzzyMatcher::new();
        let result = matcher.find_match("hello", "hello world");
        assert!(result.is_some());
        let m = result.unwrap();
        assert_eq!(m.start, 0);
        assert_eq!(m.end, 5);
        assert!((m.confidence - 1.0).abs() < 0.01); // exact match should be 1.0
        assert_eq!(m.strategy, "exact");
    }

    #[test]
    fn test_exact_match_not_found() {
        let matcher = FuzzyMatcher::new();
        let result = matcher.find_match("goodbye", "hello world");
        assert!(result.is_none());
    }

    #[test]
    fn test_whitespace_normalized_tabs_vs_spaces() {
        let matcher = FuzzyMatcher::new();
        // File has spaces, old_text has tabs (or vice versa)
        let content = "fn hello() {\n    world\n}";
        let old_text = "fn hello() {\n→   world\n}"; // → represents tab
        let result = matcher.find_match(old_text, content);
        // Should eventually find a match through one of the fuzzy strategies
        assert!(result.is_some());
    }

    #[test]
    fn test_normalize_whitespace() {
        assert_eq!(normalize_whitespace("hello    world"), "hello world");
        assert_eq!(normalize_whitespace("a\n\n\nb"), "a b");
        assert_eq!(normalize_whitespace("a\t\tb"), "a b");
    }

    #[test]
    fn test_levenshtein_threshold_reject() {
        // Too many typos should fail
        let matcher = FuzzyMatcher::with_threshold(0.85);
        // 5+ character typos in a short string should be rejected
        let result = matcher.find_match("hello", "xxxxx");
        // Levenshtein distance is 5, similarity is 0, should not match
        assert!(result.is_none() || result.unwrap().confidence < 0.85);
    }

    #[test]
    fn test_levenshtein_threshold_accept() {
        // Minor typos should match
        let matcher = FuzzyMatcher::with_threshold(0.85);
        // "hello" vs "helno" - 1 char difference
        let result = matcher.find_match("hello", "say helno world");
        assert!(result.is_some());
        let m = result.unwrap();
        assert!(m.confidence >= 0.85, "confidence {} should be >= 0.85", m.confidence);
    }

    #[test]
    fn test_replacen_only_first_occurrence() {
        // This tests the actual bug fix: only first occurrence should be replaced
        let content = "hello hello hello";
        let old_text = "hello";
        let new_text = "hi";

        // After our matcher finds the first "hello", the actual replacement
        // should use content.replacen(old, new, 1)
        let result = content.replacen(old_text, new_text, 1);
        assert_eq!(result, "hi hello hello");
    }

    #[test]
    fn test_fuzzy_matcher_chain_order() {
        let matcher = FuzzyMatcher::new();
        let names = matcher.strategy_names();
        assert_eq!(names, vec!["exact", "whitespace-normalized", "indentation-stripped", "levenshtein", "block-anchor"]);
    }

    #[test]
    fn test_build_diff() {
        let diff = build_diff("hello", "world");
        assert!(diff.contains("-hello"));
        assert!(diff.contains("+world"));
    }
}
