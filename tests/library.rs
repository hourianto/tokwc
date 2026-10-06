use std::error::Error;

use tokwc::claude::ClaudeVersion;
use tokwc::{Counts, Encoding, Metric, Provider, Tokenizer, TokenizerError};

#[test]
fn models_and_encoding_names_select_the_same_counts() {
    for (model, encoding) in [
        ("gpt-6.1-sol", Encoding::O200kBase),
        ("gpt-6-astra", Encoding::O200kBase),
        ("gpt-6-sol", Encoding::O200kBase),
        ("gpt-6-luna", Encoding::O200kBase),
        ("gpt-4o", Encoding::O200kBase),
        ("gpt-4", Encoding::Cl100kBase),
        ("gpt-oss-20b", Encoding::O200kHarmony),
        ("claude-opus-4-7", Encoding::Claude(ClaudeVersion::V4_7)),
        ("claude-opus-4-8", Encoding::Claude(ClaudeVersion::V4_8)),
        ("claude-opus-5", Encoding::Claude(ClaudeVersion::V5)),
        (
            "claude-fable-5-1",
            Encoding::Claude(ClaudeVersion::FableV5_1),
        ),
        (
            "claude-sonnet-5-5-20260921",
            Encoding::Claude(ClaudeVersion::V5_5),
        ),
    ] {
        let tokenizer = Tokenizer::for_model(model).unwrap();
        assert_eq!(tokenizer.encoding(), encoding, "{model}");
        assert_eq!(tokenizer.provider(), encoding.provider());
        let direct = Tokenizer::for_encoding(&encoding.to_string()).unwrap();
        for text in ["", "hello world\n", "é漢字😀", "\n\n", "<|endoftext|>"] {
            assert_eq!(
                tokenizer.count(text),
                direct.count(text),
                "{model} {text:?}"
            );
            if encoding.provider() == Provider::OpenAi {
                assert_eq!(
                    tokenizer.count(text),
                    tiktoken::get_encoding(encoding.name()).unwrap().count(text)
                );
            }
        }
    }
    for (alias, encoding) in [
        ("o200k", Encoding::O200kBase),
        ("cl100k", Encoding::Cl100kBase),
    ] {
        assert_eq!(Tokenizer::for_encoding(alias).unwrap().encoding(), encoding);
    }
}

#[test]
fn configuration_errors_are_typed() {
    assert!(matches!(
        Tokenizer::for_model("gpt-6.1-sol-typo"),
        Err(TokenizerError::UnknownModel(_))
    ));
    assert!(matches!(
        Tokenizer::for_encoding("made_up"),
        Err(TokenizerError::UnknownEncoding(_))
    ));
    assert!(matches!(
        Tokenizer::for_model("text-davinci-003"),
        Err(TokenizerError::UnsupportedModelEncoding { .. })
    ));
    let error = Tokenizer::for_model("claude-opus-99").unwrap_err();
    assert!(matches!(error, TokenizerError::UnsupportedClaudeModel(_)));
    assert!(error.source().is_none());
    assert!(!error.to_string().contains("--help"));
    assert!(matches!(
        Tokenizer::new(Encoding::Claude(ClaudeVersion::V5)).with_special_tokens(),
        Err(TokenizerError::UnsupportedSpecialTokens)
    ));
}

#[test]
fn special_tokens_are_explicit_and_encoding_specific() {
    for encoding in [
        Encoding::Cl100kBase,
        Encoding::O200kBase,
        Encoding::O200kHarmony,
    ] {
        let plain = Tokenizer::new(encoding);
        let special = plain.with_special_tokens().unwrap();
        assert!(!plain.special_tokens());
        assert!(special.special_tokens());
        assert!(plain.count("<|endoftext|>") > 1);
        assert_eq!(special.count("<|endoftext|>"), 1);
        assert_eq!(special, special.with_special_tokens().unwrap());
        let text = "a<|endoftext|>b <|not_a_special_token|>";
        assert_eq!(
            special.count(text),
            tiktoken::get_encoding(encoding.name())
                .unwrap()
                .count_with_special_tokens(text)
        );
    }
}

#[test]
fn selected_metrics_handle_unicode_and_empty_selections() {
    let tokenizer = Tokenizer::default();
    let text = "é\u{301}😀\tword\n";
    let all = tokenizer.count_all(text);
    assert_eq!((all.lines, all.words, all.bytes, all.chars), (1, 2, 14, 9));
    assert_eq!(all.tokens, tokenizer.count(text));
    for metric in Metric::ALL {
        let counts = tokenizer.count_metrics(text, &[metric, metric]);
        for field in Metric::ALL {
            assert_eq!(
                counts.get(field),
                if field == metric { all.get(field) } else { 0 }
            );
        }
    }
    assert_eq!(tokenizer.count_metrics(text, &[]), Counts::default());
    assert_eq!(tokenizer.count_all(""), Counts::default());
    let claude = Tokenizer::new(Encoding::Claude(ClaudeVersion::V5_5));
    assert!(claude.count_all("").tokens > 0);
    assert_eq!(
        claude.count_metrics("", &[Metric::Bytes]),
        Counts::default()
    );
}

#[test]
fn tokenizers_can_be_shared_across_threads_without_call_state() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Tokenizer>();
    assert_send_sync::<TokenizerError>();
    for model in ["gpt-6.1-sol", "claude-opus-5-5"] {
        let tokenizer = Tokenizer::for_model(model).unwrap();
        let cases = ["hello", "漢字\n", "", "after a different message"];
        let expected = cases.map(|text| tokenizer.count(text));
        std::thread::scope(|scope| {
            let handles: Vec<_> = (0..4)
                .map(|_| scope.spawn(|| cases.map(|text| tokenizer.count(text))))
                .collect();
            for handle in handles {
                assert_eq!(handle.join().unwrap(), expected);
            }
        });
    }
}
