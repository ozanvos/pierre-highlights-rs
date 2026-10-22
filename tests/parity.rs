use pierre_highlights_rs::{Highlighter, Language};
#[test]
fn all_languages_match_pierre_byte_for_byte() {
    let mut h = Highlighter::new();
    let mut covered = [false; 81];
    let mut fixtures = String::new();
    use std::io::Read;
    flate2::read::GzDecoder::new(&include_bytes!("data/parity.jsonl.gz")[..])
        .read_to_string(&mut fixtures)
        .unwrap();
    for line in fixtures.lines() {
        let case: serde_json::Value = serde_json::from_str(line).unwrap();
        let id = case["lang"].as_u64().unwrap() as u8;
        let lang = Language::from_id(id).unwrap();
        let source = case["source"].as_str().unwrap();
        let actual = h.highlight(lang, source);
        let values: Vec<u32> = actual
            .as_bytes()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|r| u32::from_le_bytes(*r))
            .collect();
        let expected: Vec<u32> = case["expected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| n.as_u64().unwrap() as u32)
            .collect();
        assert_eq!(
            values, expected,
            "case {} source {:?}",
            case["name"], source
        );
        let mut end = 0;
        for token in actual.iter() {
            assert_eq!(token.range.start, end);
            assert!(token.range.end <= source.len());
            assert!(
                source.is_char_boundary(token.range.start)
                    && source.is_char_boundary(token.range.end)
            );
            end = token.range.end;
        }
        assert_eq!(end, source.len(), "lossless {}", case["name"]);
        covered[id as usize] = true;
    }
    assert!(covered.iter().all(|&c| c));
}
#[test]
fn large_input_grows_and_can_be_reused() {
    let mut h = Highlighter::new();
    let lang = Language::from_name("typescript").unwrap();
    let source = "const value = 'λ😀'; // comment\n".repeat(100_000);
    assert_eq!(
        h.highlight(lang, &source).iter().last().unwrap().range.end,
        source.len()
    );
    assert_eq!(
        h.highlight(lang, "const x = 1;")
            .iter()
            .last()
            .unwrap()
            .range
            .end,
        12
    );
    assert!(h.highlight(lang, "").is_empty());
}

#[test]
fn catalog_preserves_every_upstream_alias() {
    let aliases: serde_json::Value =
        serde_json::from_str(include_str!("data/aliases.json")).unwrap();
    for (alias, id) in aliases.as_object().unwrap() {
        let language = Language::from_id(id.as_u64().unwrap() as u8).unwrap();
        assert_eq!(Language::from_name(alias), Some(language));
        assert_eq!(
            Language::from_name(&alias.to_ascii_uppercase()),
            Some(language)
        );
    }
    for language in Language::ALL {
        assert_eq!(Language::from_name(language.name()), Some(language));
    }
    assert!(Language::from_id(81).is_none());
    assert!(Language::from_name("not-a-language").is_none());
    assert!(pierre_highlights_rs::TokenKind::from_id(73).is_none());
}
