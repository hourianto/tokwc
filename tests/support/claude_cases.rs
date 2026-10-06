// Deterministic Unicode and boundary inputs shared by regression tests.
pub fn visit(mut check: impl FnMut(&str)) {
    const ATOMS: &[&str] = &[
        "",
        " ",
        "  ",
        "\t",
        "\n",
        "\r\n",
        "\0",
        "\u{1}\u{2}\u{3}\u{4}",
        "a",
        "A",
        "ABC",
        "Hello",
        "camelCase",
        "'",
        "''",
        "'s",
        "'re",
        "’",
        ".",
        "././",
        "1",
        "1234",
        "١۲",
        "²",
        "Ⅳ",
        "ⓐ",
        "\u{a0}",
        "\u{3000}",
        "\u{200b}",
        "\u{e000}",
        "é",
        "e\u{301}",
        "\u{301}",
        "\u{345}",
        "\u{711}",
        "Ω",
        "ϴ",
        "ϒ",
        "İ",
        "ẞ",
        "\u{1c89}",
        "\u{a7cb}",
        "क",
        "्",
        "क्ष",
        "ทํางาน",
        "ฺ",
        "漢",
        "〱",
        "〄",
        "ヲ",
        "ւ",
        "한",
        "\u{fe0f}",
        "🙂",
        "👩‍💻",
        "\u{10000}",
        "\u{10ffff}",
    ];
    for a in ATOMS {
        for b in ATOMS {
            for c in ["", " ", "'s", "A", "\u{301}", "\u{fe0f}", "\n\n"] {
                check(&format!("{a}{b}{c}"));
            }
        }
    }

    // Fixed xorshift state makes failures reproducible, without a random-number dependency.
    let mut random = 0x1234_5678u32;
    let mut next = || {
        random ^= random << 13;
        random ^= random >> 17;
        random ^= random << 5;
        random
    };
    for _ in 0..10_000 {
        let mut text = String::new();
        for _ in 0..next() % 64 {
            let value = next();
            if value % 4 != 0 {
                text.push_str(ATOMS[value as usize % ATOMS.len()]);
            } else if let Some(ch) = char::from_u32(next() % 0x0011_0000) {
                text.push(ch);
            }
        }
        check(&text);
    }

    // Cover every Unicode scalar, including unassigned codepoints, in mixed-boundary batches.
    let mut text = String::new();
    for cp in 0..=0x0010_ffff {
        if let Some(ch) = char::from_u32(cp) {
            text.push_str(" A'");
            text.push(ch);
            text.push_str("'s\n");
        }
        if cp % 256 == 255 {
            check(&text);
            text.clear();
        }
    }

    for length in 0..=129 {
        for whitespace in [' ', '\t', '\n', '\r', '\u{a0}'] {
            let run: String = std::iter::repeat_n(whitespace, length).collect();
            check(&run);
            check(&format!("Hello{run}'s{run}"));
            check(&format!("{run}Hello{run}\u{a0}"));
        }
    }
}
