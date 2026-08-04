use chrono::{SecondsFormat, Utc};
use fake::faker::internet::en::{SafeEmail, Username};
use fake::faker::lorem::en::Word;
use fake::faker::name::en::Name;
use fake::faker::phone_number::en::PhoneNumber;
use fake::Fake;
use rand::Rng;
use uuid::Uuid;

/// Normalize field names for heuristic matching (snake_case, lowercase).
pub fn normalize_field(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c == '-' || c == ' ' {
            out.push('_');
            continue;
        }
        if c.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

pub fn string_for_field(field_name: &str, format: Option<&str>) -> String {
    if let Some(fmt) = format {
        let fmt_l = fmt.to_ascii_lowercase();
        // Handle Debug-style format variants from openapiv3 like `DateTime`
        if fmt_l.contains("uuid") {
            return Uuid::new_v4().to_string();
        }
        if fmt_l.contains("date-time") || fmt_l.contains("datetime") {
            return Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
        }
        if fmt_l.contains("date") {
            return Utc::now().format("%Y-%m-%d").to_string();
        }
        if fmt_l.contains("email") {
            return SafeEmail().fake();
        }
        if fmt_l.contains("uri") || fmt_l.contains("url") {
            return format!("https://example.com/{}", Word().fake::<String>());
        }
    }

    let n = normalize_field(field_name);

    if n.contains("email") {
        return SafeEmail().fake();
    }
    if n == "uuid" || n.ends_with("_uuid") {
        return Uuid::new_v4().to_string();
    }
    if n == "id" || n.ends_with("_id") {
        // String id fields get UUIDs
        return Uuid::new_v4().to_string();
    }
    if n.contains("phone") || n == "tel" || n.ends_with("_tel") {
        return PhoneNumber().fake();
    }
    if n.contains("avatar") || n.contains("image") {
        return format!("https://picsum.photos/seed/{}/200", Uuid::new_v4());
    }
    if n.contains("url") || n.contains("uri") || n.contains("href") || n.contains("link") {
        return format!("https://example.com/{}", Word().fake::<String>());
    }
    if n.ends_with("_at") || n == "created_at" || n == "updated_at" || n == "deleted_at" {
        return Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    }
    if n.ends_with("_date") || n == "date" || n.contains("birthday") || n.contains("birth_date") {
        return Utc::now().format("%Y-%m-%d").to_string();
    }
    if n == "username" || n.ends_with("_username") {
        return Username().fake();
    }
    if n.contains("name") {
        return Name().fake();
    }
    if n.contains("title") {
        return Word().fake::<String>();
    }
    if n.contains("description") || n.contains("bio") || n.contains("body") || n.contains("content")
    {
        return fake::faker::lorem::en::Sentence(4..12).fake::<String>();
    }
    if n.contains("price") || n.ends_with("_jpy") || n.contains("amount") || n.contains("cost") {
        let mut rng = rand::thread_rng();
        return format!("{}", rng.gen_range(100..50_000));
    }

    Word().fake::<String>()
}

pub fn integer_for_field(field_name: &str, format: Option<&str>) -> i64 {
    let n = normalize_field(field_name);
    let mut rng = rand::thread_rng();

    if let Some(fmt) = format {
        let fmt_l = fmt.to_ascii_lowercase();
        if fmt_l.contains("int64") {
            return rng.gen_range(1..1_000_000);
        }
    }

    if n == "id" || n.ends_with("_id") {
        return rng.gen_range(1..10_000);
    }
    if n.contains("price") || n.ends_with("_jpy") || n.contains("amount") || n.contains("cost") {
        return rng.gen_range(100..50_000);
    }
    if n.contains("age") {
        return rng.gen_range(18..80);
    }
    if n.contains("count") || n.contains("qty") || n.contains("quantity") || n.contains("stock") {
        return rng.gen_range(0..500);
    }
    if n.contains("year") {
        return rng.gen_range(1990..2026);
    }

    rng.gen_range(0..10_000)
}

pub fn number_for_field(field_name: &str) -> f64 {
    let n = normalize_field(field_name);
    let mut rng = rand::thread_rng();

    if n.contains("price") || n.ends_with("_jpy") || n.contains("amount") || n.contains("cost") {
        let v: f64 = rng.gen_range(100.0..50_000.0);
        return (v * 100.0).round() / 100.0;
    }
    if n.contains("rate") || n.contains("ratio") || n.contains("percent") {
        let v: f64 = rng.gen_range(0.0..100.0);
        return (v * 100.0).round() / 100.0;
    }
    let v: f64 = rng.gen_range(0.0..1000.0);
    (v * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_heuristic() {
        let v = string_for_field("user_email", None);
        assert!(v.contains('@'));
    }

    #[test]
    fn normalize_camel() {
        assert_eq!(normalize_field("userEmail"), "user_email");
        assert_eq!(normalize_field("createdAt"), "created_at");
    }
}
