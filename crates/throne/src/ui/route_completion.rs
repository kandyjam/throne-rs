//! Autocomplete for simple route rules — mirrors upstream `AutoCompleteTextEdit`
//! + `QStringList` of rule type prefixes and `ruleset:<tag>` entries.
//!
//! Upstream behavior (QvAutoCompleteTextEdit):
//! - Completes against the **line under the cursor**
//! - Case-insensitive **contains** match
//! - Accepting a suggestion **replaces the whole line**

use std::rc::Rc;
use std::sync::OnceLock;

use anyhow::Result;
use gpui::{Context, Task, Window};
use gpui_component::input::{CompletionProvider, InputState, RopeExt};
use gpui_component::Rope;
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionResponse, CompletionTextEdit, Position, TextEdit,
};
use throne_core_client::RULE_SET_LIST;

/// Rule-type prefixes shown for Route Profile simple rules (Direct / Proxy / …).
const SIMPLE_RULE_PREFIXES: &[&str] = &[
    "domain:",
    "suffix:",
    "regex:",
    "keyword:",
    "ip:",
    "processName:",
    "processPath:",
    "ruleset:",
];

/// Prefixes for the main Routes → DNS rules editor (upstream is a smaller set).
const DNS_RULE_PREFIXES: &[&str] = &["domain:", "suffix:", "regex:", "ruleset:"];

const MAX_ITEMS: usize = 60;

/// Completion catalog: type prefixes + every `ruleset:<tag>` from the well-known list.
fn simple_rule_candidates() -> &'static [String] {
    static C: OnceLock<Vec<String>> = OnceLock::new();
    C.get_or_init(|| build_candidates(SIMPLE_RULE_PREFIXES))
}

fn dns_rule_candidates() -> &'static [String] {
    static C: OnceLock<Vec<String>> = OnceLock::new();
    C.get_or_init(|| build_candidates(DNS_RULE_PREFIXES))
}

fn build_candidates(prefixes: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = prefixes.iter().map(|s| (*s).to_string()).collect();
    v.reserve(RULE_SET_LIST.len());
    for (tag, _) in RULE_SET_LIST {
        v.push(format!("ruleset:{tag}"));
    }
    v
}

/// Shared filter / line-replace logic for rule-line completion.
struct RuleLineProvider {
    candidates: &'static [String],
}

impl RuleLineProvider {
    fn line_bounds(text: &Rope, offset: usize) -> (usize, usize, String) {
        let pos = text.offset_to_position(offset);
        let row = pos.line as usize;
        let start = text.line_start_offset(row);
        // `line_end_offset` is the exclusive end of content (before trailing `\n`).
        let end = text.line_end_offset(row);
        let end = end.min(text.len());
        let start = start.min(end);
        let line = text.slice(start..end).to_string();
        // Trim CR from `\r\n` line endings if present.
        let line = line.trim_end_matches('\r').to_string();
        let end = start + line.len();
        (start, end, line)
    }

    fn filter_items(&self, query: &str) -> Vec<CompletionItem> {
        let q = query.trim();
        let q_lower = q.to_ascii_lowercase();

        // Empty line → only the short type-prefix list (avoid dumping 2k rulesets).
        if q.is_empty() {
            return self
                .candidates
                .iter()
                .filter(|c| !c.starts_with("ruleset:") || *c == "ruleset:")
                .take(MAX_ITEMS)
                .map(|label| completion_item(label, 0, 0))
                .collect();
        }

        // Prefer starts-with, then contains (MatchContains), case-insensitive.
        let mut starts: Vec<&String> = Vec::new();
        let mut contains: Vec<&String> = Vec::new();
        for c in self.candidates {
            let cl = c.to_ascii_lowercase();
            if cl.starts_with(&q_lower) {
                starts.push(c);
            } else if cl.contains(&q_lower) {
                contains.push(c);
            }
            if starts.len() + contains.len() >= MAX_ITEMS * 2 {
                break;
            }
        }
        starts
            .into_iter()
            .chain(contains)
            .take(MAX_ITEMS)
            .map(|label| completion_item(label, 0, 0))
            .collect()
    }
}

fn completion_item(label: &str, start_line: u32, start_col: u32) -> CompletionItem {
    // Range is filled in `completions` once we know line bounds.
    let _ = (start_line, start_col);
    CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::VALUE),
        detail: None,
        insert_text: Some(label.to_string()),
        ..Default::default()
    }
}

impl CompletionProvider for RuleLineProvider {
    fn completions(
        &self,
        text: &Rope,
        offset: usize,
        _trigger: lsp_types::CompletionContext,
        _window: &mut Window,
        _cx: &mut Context<InputState>,
    ) -> Task<Result<CompletionResponse>> {
        let (line_start, line_end, line) = Self::line_bounds(text, offset);
        let mut items = self.filter_items(&line);

        let start_pos = text.offset_to_position(line_start);
        let end_pos = text.offset_to_position(line_end);
        for item in &mut items {
            let new_text = item.label.clone();
            item.text_edit = Some(CompletionTextEdit::Edit(TextEdit {
                range: lsp_types::Range {
                    start: Position {
                        line: start_pos.line,
                        character: start_pos.character,
                    },
                    end: Position {
                        line: end_pos.line,
                        character: end_pos.character,
                    },
                },
                new_text,
            }));
        }

        Task::ready(Ok(CompletionResponse::Array(items)))
    }

    fn is_completion_trigger(
        &self,
        _offset: usize,
        new_text: &str,
        _cx: &mut Context<InputState>,
    ) -> bool {
        // Don't open the menu when the user commits a new line.
        !new_text.contains('\n') && !new_text.contains('\r')
    }
}

/// Provider for Route Profile simple-rule text areas.
pub fn simple_rule_completion_provider() -> Rc<dyn CompletionProvider> {
    Rc::new(RuleLineProvider {
        candidates: simple_rule_candidates(),
    })
}

/// Provider for Routes → DNS rules text area.
pub fn dns_rule_completion_provider() -> Rc<dyn CompletionProvider> {
    Rc::new(RuleLineProvider {
        candidates: dns_rule_candidates(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_only_type_prefixes() {
        let p = RuleLineProvider {
            candidates: simple_rule_candidates(),
        };
        let items = p.filter_items("");
        assert!(!items.is_empty());
        assert!(items.iter().all(|i| {
            SIMPLE_RULE_PREFIXES.contains(&i.label.as_str()) || i.label == "ruleset:"
        }));
        assert!(items.len() <= SIMPLE_RULE_PREFIXES.len());
    }

    #[test]
    fn suffix_prefix_matches() {
        let p = RuleLineProvider {
            candidates: simple_rule_candidates(),
        };
        let items = p.filter_items("suf");
        assert!(items.iter().any(|i| i.label == "suffix:"));
    }

    #[test]
    fn ruleset_tag_contains_match() {
        let p = RuleLineProvider {
            candidates: simple_rule_candidates(),
        };
        let items = p.filter_items("geoip-cn");
        assert!(
            items.iter().any(|i| i.label == "ruleset:geoip-cn"),
            "expected ruleset:geoip-cn in {:?}",
            items.iter().map(|i| &i.label).collect::<Vec<_>>()
        );
    }

    #[test]
    fn case_insensitive() {
        let p = RuleLineProvider {
            candidates: simple_rule_candidates(),
        };
        let items = p.filter_items("DOMAIN");
        assert!(items.iter().any(|i| i.label == "domain:"));
    }
}
