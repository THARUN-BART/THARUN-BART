use chrono::{Datelike, Local, NaiveDate};

/// Calculates the calendar age string: "XX years, XX months, XX days [🎂]"
pub fn daily_readme(birthday: NaiveDate) -> String {
    let today = Local::now().date_naive();
    calculate_diff(today, birthday)
}

/// Core date difference logic with borrowing
pub fn calculate_diff(today: NaiveDate, birthday: NaiveDate) -> String {
    let mut years = today.year() - birthday.year();
    let mut months = today.month() as i32 - birthday.month() as i32;
    let mut days = today.day() as i32 - birthday.day() as i32;

    // Borrow days from the previous month if days < 0
    if days < 0 {
        months -= 1;
        // Last day of the previous month
        let first_of_this_month = NaiveDate::from_ymd_opt(today.year(), today.month(), 1).unwrap();
        let last_of_prev_month = first_of_this_month.pred_opt().unwrap();
        days += last_of_prev_month.day() as i32;
    }

    // Borrow months from years if months < 0
    if months < 0 {
        years -= 1;
        months += 12;
    }

    let years = years.max(0) as u32;
    let months = months.max(0) as u32;
    let days = days.max(0) as u32;

    let format_unit = |value: u32, unit: &str| -> String {
        if value == 1 {
            format!("{} {}", value, unit)
        } else {
            format!("{} {}s", value, unit)
        }
    };

    let cake = if months == 0 && days == 0 { " 🎂" } else { "" };

    format!(
        "{}, {}, {}{}",
        format_unit(years, "year"),
        format_unit(months, "month"),
        format_unit(days, "day"),
        cake
    )
}
