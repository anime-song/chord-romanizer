use chord_romanizer::{
    ParsedSymbol, ProgressionItem, RomanizedChord, Romanizer, RomanizerOptions, SpelledNote,
    parse_chord,
};

const CASES: &str = include_str!("fixtures/compat_cases.txt");
const PYTHON_GOLDEN: &str = include_str!("fixtures/python_golden.jsonl");

#[test]
fn every_python_analysis_scenario_matches_golden_output() {
    let case_lines: Vec<_> = CASES.lines().collect();
    let golden_lines: Vec<_> = PYTHON_GOLDEN.lines().collect();
    assert_eq!(case_lines.len(), golden_lines.len());
    assert!(
        case_lines.len() >= 57,
        "compatibility suite unexpectedly shrank"
    );

    for (manifest, expected) in case_lines.into_iter().zip(golden_lines) {
        let mut fields = manifest.splitn(4, '|');
        let name = fields.next().expect("case name");
        let default_tonic = fields.next().expect("default tonic");
        let simplify = fields.next().expect("simplify flag") == "1";
        let item_list = fields.next().expect("item list");

        let mut options = RomanizerOptions::python_019(default_tonic).unwrap();
        options.simplify_accidentals = simplify;
        let romanizer = Romanizer::with_options(options).unwrap();
        let progression: Vec<_> = item_list
            .split(';')
            .map(|encoded| {
                if let Some((symbol, tonic)) = encoded.split_once('~') {
                    ProgressionItem::in_key(
                        parse_chord(symbol).unwrap(),
                        SpelledNote::parse(tonic).unwrap(),
                    )
                } else {
                    ProgressionItem::new(parse_chord(encoded).unwrap())
                }
            })
            .collect();
        let results = romanizer.annotate_progression(&progression);
        let actual = case_json(name, &results);
        assert_eq!(actual, expected, "Python compatibility mismatch in {name}");
    }
}

#[test]
fn parser_scenarios_from_python_suite_are_covered() {
    let cases = [
        ("C", "C", "", None),
        ("C#m7/G#", "C#", "m7", Some("G#")),
        ("F", "F", "", None),
        ("F#m7-5", "F#", "m7-5", None),
        ("G7/B", "G", "7", Some("B")),
        ("Db/F", "Db", "", Some("F")),
        ("Bbb7", "Bbb", "7", None),
        ("F##m7", "F##", "m7", None),
    ];
    for (symbol, root, quality, bass) in cases {
        let ParsedSymbol::Chord(chord) = parse_chord(symbol).unwrap() else {
            panic!("expected chord for {symbol}");
        };
        assert_eq!(chord.root_lexeme, root, "root for {symbol}");
        assert_eq!(chord.quality, quality, "quality for {symbol}");
        assert_eq!(chord.bass_lexeme.as_deref(), bass, "bass for {symbol}");
    }

    let ParsedSymbol::Chord(chord) = parse_chord("C7/Bbb").unwrap() else {
        panic!("expected slash chord");
    };
    assert_eq!(chord.bass_lexeme.as_deref(), Some("Bbb"));
}

fn case_json(name: &str, results: &[RomanizedChord]) -> String {
    format!(
        "{{\"case\":{},\"results\":[{}]}}",
        json_string(name),
        results
            .iter()
            .map(result_json)
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn result_json(result: &RomanizedChord) -> String {
    // Key order matches json.dumps(sort_keys=True) in the Python generator.
    format!(
        concat!(
            "{{\"alter\":{},",
            "\"alternate_labels\":{},",
            "\"degree_bass\":{},",
            "\"degree_root\":{},",
            "\"is_hybrid\":{},",
            "\"is_ii_v_start\":{},",
            "\"is_resolution_target\":{},",
            "\"resolution_type\":{},",
            "\"roman\":{},",
            "\"roman_root_bass\":{},",
            "\"symbol_fixed\":{}}}"
        ),
        option_json(result.alter.as_deref()),
        string_array_json(&result.alternate_labels),
        option_owned_json(result.degree_bass.map(|value| value.to_string())),
        json_string(&result.degree_root.to_string()),
        bool_json(result.is_hybrid),
        bool_json(result.is_ii_v_start),
        bool_json(result.is_resolution_target),
        option_json(result.resolution_type.map(|value| value.as_str())),
        json_string(&result.roman),
        option_json(result.roman_root_bass.as_deref()),
        json_string(&result.symbol_fixed),
    )
}

fn bool_json(value: bool) -> &'static str {
    if value { "true" } else { "false" }
}

fn option_json(value: Option<&str>) -> String {
    value.map(json_string).unwrap_or_else(|| "null".to_owned())
}

fn option_owned_json(value: Option<String>) -> String {
    value
        .as_deref()
        .map(json_string)
        .unwrap_or_else(|| "null".to_owned())
}

fn string_array_json(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|value| json_string(value))
            .collect::<Vec<_>>()
            .join(",")
    )
}

fn json_string(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\u{08}' => output.push_str("\\b"),
            '\u{0c}' => output.push_str("\\f"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character < '\u{20}' => {
                output.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}
