use github_stats_rs::svg::{calculate_dots, format_number, format_number_i64, justify_format};

#[test]
fn test_format_numbers() {
    assert_eq!(format_number(0), "0");
    assert_eq!(format_number(42), "42");
    assert_eq!(format_number(1000), "1,000");
    assert_eq!(format_number(1128842), "1,128,842");
    assert_eq!(format_number_i64(-500), "-500");
    assert_eq!(format_number_i64(483575), "483,575");
}

#[test]
fn test_calculate_dots_alignment() {
    assert_eq!(calculate_dots("1,128,842", 0), "");
    assert_eq!(calculate_dots("483,575", 9), ". ");
    assert_eq!(calculate_dots("357", 22), " ................... ");
}

#[test]
fn test_replace_svg_tag_content() {
    let sample = r#"<tspan id="age_data_dots"></tspan><tspan id="age_data">old age</tspan>"#;
    let mut s = sample.to_string();
    justify_format(&mut s, "age_data", "19 years", 12);
    assert_eq!(s, r#"<tspan id="age_data_dots"> .... </tspan><tspan id="age_data">19 years</tspan>"#);
}
