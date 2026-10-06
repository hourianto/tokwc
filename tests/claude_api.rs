use std::collections::BTreeMap;

use serde::Deserialize;
use tokwc::Tokenizer;

#[derive(Deserialize)]
struct Case {
    id: String,
    text: String,
    counts: BTreeMap<String, usize>,
}

#[test]
fn matches_recorded_anthropic_counts() {
    // Expected counts come from Anthropic's count_tokens endpoint, with each text sent as
    // one user message (API version 2023-06-01). Inputs are synthetic; no request metadata is kept.
    // Opus/Sonnet 5.5 readings were recorded on 2026-10-01.
    for line in include_str!("fixtures/claude_api.jsonl").lines() {
        let case: Case = serde_json::from_str(line).unwrap();
        for (model, expected) in case.counts {
            assert_eq!(
                Tokenizer::for_model(&model).unwrap().count(&case.text),
                expected,
                "{} / {model}: {:?}",
                case.id,
                case.text
            );
        }
    }
}
