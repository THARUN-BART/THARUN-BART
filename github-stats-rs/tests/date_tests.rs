use chrono::NaiveDate;
use github_stats_rs::date::calculate_diff;

#[test]
fn test_birthday_today() {
    let birthday = NaiveDate::from_ymd_opt(2006, 11, 29).unwrap();
    let today = NaiveDate::from_ymd_opt(2026, 11, 29).unwrap();
    assert_eq!(calculate_diff(today, birthday), "20 years, 0 months, 0 days 🎂");
}

#[test]
fn test_regular_date_borrowing() {
    let birthday = NaiveDate::from_ymd_opt(2006, 11, 29).unwrap();
    let today = NaiveDate::from_ymd_opt(2026, 11, 14).unwrap();
    assert_eq!(calculate_diff(today, birthday), "19 years, 11 months, 16 days");
}

#[test]
fn test_singular_plural() {
    let birthday = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
    let today = NaiveDate::from_ymd_opt(2026, 2, 2).unwrap();
    assert_eq!(calculate_diff(today, birthday), "1 year, 1 month, 1 day");
}
