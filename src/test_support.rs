use chrono::{NaiveDate, NaiveDateTime};

pub fn fixed_now() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 10, 18)
        .and_then(|date| date.and_hms_opt(12, 0, 0))
        .expect("data válida")
}

pub fn morning_now() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 10, 17)
        .and_then(|date| date.and_hms_opt(8, 30, 0))
        .expect("data válida")
}

pub fn recommendation_request_now() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 10, 19)
        .and_then(|date| date.and_hms_opt(9, 0, 0))
        .expect("data válida")
}

pub fn widget_box_now() -> NaiveDateTime {
    NaiveDate::from_ymd_opt(2026, 10, 17)
        .and_then(|date| date.and_hms_opt(9, 41, 0))
        .expect("data válida")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_dates_have_the_expected_values() {
        assert_eq!(fixed_now().to_string(), "2026-10-18 12:00:00");
        assert_eq!(morning_now().to_string(), "2026-10-17 08:30:00");
        assert_eq!(
            recommendation_request_now().to_string(),
            "2026-10-19 09:00:00"
        );
        assert_eq!(widget_box_now().to_string(), "2026-10-17 09:41:00");
    }
}
