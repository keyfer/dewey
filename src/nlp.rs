use chrono::{Datelike, Local, NaiveDate, Weekday};

use crate::error::Result;
use crate::model::Priority;

pub fn parse_quick_add(
    text: &str,
    default_backend: Option<&str>,
    valid_backends: &[&str],
) -> Result<(
    String,
    Priority,
    Option<NaiveDate>,
    Vec<String>,
    String,
    Option<String>,
)> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let mut tags = Vec::new();
    let mut priority = Priority::None;
    let mut due: Option<NaiveDate> = None;
    let mut backend: Option<String> = None;
    let mut project: Option<String> = None;
    let mut title_words = Vec::new();

    let today = Local::now().date_naive();

    let mut i = 0;
    while i < words.len() {
        let word = words[i];

        if word.starts_with('@') && word.len() > 1 && backend.is_none() {
            let candidate = &word[1..];
            if valid_backends.is_empty() || valid_backends.iter().any(|&k| k.eq_ignore_ascii_case(candidate)) {
                backend = Some(candidate.to_string());
                i += 1;
                continue;
            }
        }

        if word.starts_with('#') {
            tags.push(word[1..].to_string());
            i += 1;
            continue;
        }

        if word.starts_with('+') && word.len() > 1 && project.is_none() {
            project = Some(word[1..].to_string());
            i += 1;
            continue;
        }

        if word == "(p1)" {
            priority = Priority::High;
            i += 1;
            continue;
        }
        if word == "(p2)" {
            priority = Priority::Medium;
            i += 1;
            continue;
        }
        if word == "(p3)" {
            priority = Priority::Low;
            i += 1;
            continue;
        }

        let lower = word.to_lowercase();
        if let Some(date) = try_parse_date(&lower, word, &words, i, today, &mut title_words) {
            due = Some(date);
            i += 1;
            continue;
        }

        title_words.push(word);
        i += 1;
    }

    let title = title_words.join(" ");

    let backend = backend.unwrap_or_else(|| {
        default_backend.unwrap_or("local").to_string()
    });

    Ok((title, priority, due, tags, backend, project))
}

fn parse_weekday(day: &str, today: NaiveDate) -> Option<NaiveDate> {
    let target_weekday = match day {
        "monday" | "mon" => Weekday::Mon,
        "tuesday" | "tue" | "tues" => Weekday::Tue,
        "wednesday" | "wed" => Weekday::Wed,
        "thursday" | "thu" | "thurs" => Weekday::Thu,
        "friday" | "fri" => Weekday::Fri,
        "saturday" | "sat" => Weekday::Sat,
        "sunday" | "sun" => Weekday::Sun,
        _ => return None,
    };

    let today_weekday = today.weekday();
    let days_until = (target_weekday.num_days_from_monday() as i64
        - today_weekday.num_days_from_monday() as i64
        + 7)
        % 7;
    let days_until = if days_until == 0 { 7 } else { days_until };

    Some(today + chrono::Duration::days(days_until))
}

fn try_parse_date(
    lower: &str,
    word: &str,
    words: &[&str],
    idx: usize,
    today: NaiveDate,
    title_words: &mut Vec<&str>,
) -> Option<NaiveDate> {
    match lower {
        "today" => return Some(today),
        "tomorrow" | "tmr" => return Some(today + chrono::Duration::days(1)),
        _ => {}
    }

    if idx > 0 {
        let prev = words[idx - 1].to_lowercase();
        if (prev == "on" || prev == "by") && !title_words.is_empty() {
            if let Some(date) = parse_weekday(lower, today) {
                title_words.pop(); // Remove "on" or "by"
                return Some(date);
            }
        }
    }

    if let Some(date) = parse_weekday(lower, today) {
        return Some(date);
    }

    NaiveDate::parse_from_str(word, "%Y-%m-%d").ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_task() {
        let (title, priority, due, tags, _, _) = parse_quick_add("Buy milk", None, &[]).unwrap();
        assert_eq!(title, "Buy milk");
        assert_eq!(priority, Priority::None);
        assert!(due.is_none());
        assert!(tags.is_empty());
    }

    #[test]
    fn test_parse_with_tags() {
        let (title, _, _, tags, _, _) = parse_quick_add("Buy milk #groceries #shopping", None, &[]).unwrap();
        assert_eq!(title, "Buy milk");
        assert_eq!(tags, vec!["groceries", "shopping"]);
    }

    #[test]
    fn test_parse_with_priority_p1() {
        let (_, priority, _, _, _, _) = parse_quick_add("Call dentist (p1)", None, &[]).unwrap();
        assert_eq!(priority, Priority::High);
    }

    #[test]
    fn test_parse_with_priority_p2() {
        let (_, priority, _, _, _, _) = parse_quick_add("Submit report (p2)", None, &[]).unwrap();
        assert_eq!(priority, Priority::Medium);
    }

    #[test]
    fn test_parse_with_priority_p3() {
        let (_, priority, _, _, _, _) = parse_quick_add("Buy groceries (p3)", None, &[]).unwrap();
        assert_eq!(priority, Priority::Low);
    }

    #[test]
    fn test_parse_with_priority_p123() {
        let (_, priority_p1, _, _, _, _) = parse_quick_add("Important task (p1)", None, &[]).unwrap();
        assert_eq!(priority_p1, Priority::High);

        let (_, priority_p2, _, _, _, _) = parse_quick_add("Medium task (p2)", None, &[]).unwrap();
        assert_eq!(priority_p2, Priority::Medium);

        let (_, priority_p3, _, _, _, _) = parse_quick_add("Low task (p3)", None, &[]).unwrap();
        assert_eq!(priority_p3, Priority::Low);
    }

    #[test]
    fn test_parse_due_today() {
        let (_, _, due, _, _, _) = parse_quick_add("Call mom today", None, &[]).unwrap();
        let today = Local::now().date_naive();
        assert_eq!(due, Some(today));
    }

    #[test]
    fn test_parse_due_tomorrow() {
        let (_, _, due, _, _, _) = parse_quick_add("Submit report tomorrow", None, &[]).unwrap();
        let tomorrow = Local::now().date_naive() + chrono::Duration::days(1);
        assert_eq!(due, Some(tomorrow));
    }

    #[test]
    fn test_parse_due_tmr() {
        let (_, _, due, _, _, _) = parse_quick_add("Buy milk tmr", None, &[]).unwrap();
        let tomorrow = Local::now().date_naive() + chrono::Duration::days(1);
        assert_eq!(due, Some(tomorrow));
    }

    #[test]
    fn test_parse_due_specific_date() {
        let (_, _, due, _, _, _) = parse_quick_add("Meeting 2025-03-15", None, &[]).unwrap();
        assert_eq!(
            due,
            Some(chrono::NaiveDate::from_ymd_opt(2025, 3, 15).unwrap())
        );
    }

    #[test]
    fn test_parse_combined() {
        let (title, priority, due, tags, backend, _) =
            parse_quick_add("Review PR #work (p1) tomorrow @linear", None, &[]).unwrap();

        assert_eq!(title, "Review PR");
        assert_eq!(priority, Priority::High);
        assert!(due.is_some());
        assert_eq!(tags, vec!["work"]);
        assert_eq!(backend, "linear");
    }

    #[test]
    fn test_parse_with_project() {
        let (title, _, _, tags, _, project) =
            parse_quick_add("Fix login bug #auth +onboarding (p1) @linear", None, &[]).unwrap();
        assert_eq!(title, "Fix login bug");
        assert_eq!(tags, vec!["auth"]);
        assert_eq!(project, Some("onboarding".to_string()));
    }

    #[test]
    fn test_try_parse_date_keywords() {
        let today = Local::now().date_naive();

        let words: Vec<&str> = vec![];
        let mut title_words: Vec<&str> = vec![];
        assert_eq!(
            try_parse_date("today", "today", &words, 0, today, &mut title_words),
            Some(today)
        );

        let words: Vec<&str> = vec![];
        let mut title_words: Vec<&str> = vec![];
        assert_eq!(
            try_parse_date("tomorrow", "tomorrow", &words, 0, today, &mut title_words),
            Some(today + chrono::Duration::days(1))
        );

        let words: Vec<&str> = vec![];
        let mut title_words: Vec<&str> = vec![];
        assert_eq!(
            try_parse_date("tmr", "tmr", &words, 0, today, &mut title_words),
            Some(today + chrono::Duration::days(1))
        );
    }

    #[test]
    fn test_try_parse_date_iso() {
        let today = Local::now().date_naive();

        let words: Vec<&str> = vec![];
        let mut title_words: Vec<&str> = vec![];
        assert_eq!(
            try_parse_date(
                "2025-03-15",
                "2025-03-15",
                &words,
                0,
                today,
                &mut title_words
            ),
            Some(chrono::NaiveDate::from_ymd_opt(2025, 3, 15).unwrap())
        );

        let words: Vec<&str> = vec![];
        let mut title_words: Vec<&str> = vec![];
        assert_eq!(
            try_parse_date("invalid", "invalid", &words, 0, today, &mut title_words),
            None
        );
    }

    #[test]
    fn test_try_parse_date_with_preposition() {
        let today = Local::now().date_naive();
        let mut title_words: Vec<&str> = vec!["Meeting"];
        let words: Vec<&str> = vec!["Meeting", "on", "monday"];

        let result = try_parse_date("monday", "monday", &words, 2, today, &mut title_words);
        assert!(result.is_some());
        assert!(title_words.is_empty());
    }

    #[test]
    fn test_parse_backend_routing() {
        let (_, _, _, _, backend, _) = parse_quick_add("Task @linear", None, &[]).unwrap();
        assert_eq!(backend, "linear");
    }

    #[test]
    fn test_parse_backend_routing_linear() {
        let (title, priority, due, _, backend, _) =
            parse_quick_add("Fix bug (p1) tomorrow @linear", None, &[]).unwrap();
        assert_eq!(backend, "linear");
        assert!(title.contains("Fix bug"));
        assert_eq!(priority, Priority::High);
        assert!(due.is_some());
    }

    #[test]
    fn test_parse_default_backend() {
        let (_, _, _, _, backend, _) = parse_quick_add("Simple task", None, &[]).unwrap();
        assert_eq!(backend, "local");
    }

    #[test]
    fn test_parse_default_backend_configured_linear() {
        let (_, _, _, _, backend, _) = parse_quick_add("Simple task", Some("linear"), &[]).unwrap();
        assert_eq!(backend, "linear");
    }

    #[test]
    fn test_parse_explicit_backend_overrides_default() {
        let (_, _, _, _, backend, _) = parse_quick_add("Task @local", Some("linear"), &[]).unwrap();
        assert_eq!(backend, "local");
    }

    #[test]
    fn test_parse_named_backend() {
        let (_, _, _, _, backend, _) = parse_quick_add("Fix bug @work", None, &[]).unwrap();
        assert_eq!(backend, "work");
    }

    #[test]
    fn test_parse_named_backend_personal() {
        let (title, _, _, _, backend, _) = parse_quick_add("Buy milk @personal", None, &[]).unwrap();
        assert_eq!(title, "Buy milk");
        assert_eq!(backend, "personal");
    }
}
