//! Record and domain object builders for tests.

use swe_gateway::saf::database::Record;
use swe_gateway::saf::notification::{Notification, NotificationChannel};
use swe_gateway::saf::payment::{Currency, Customer, Money};

// =============================================================================
// Database Records
// =============================================================================

/// Build a record with `id` and `name` fields.
pub fn record(id: &str, name: &str) -> Record {
    let mut r = serde_json::Map::new();
    r.insert("id".into(), serde_json::json!(id));
    r.insert("name".into(), serde_json::json!(name));
    r
}

/// Build a record with `id`, `name`, and `status` fields.
pub fn record_with_status(id: &str, name: &str, status: &str) -> Record {
    let mut r = record(id, name);
    r.insert("status".into(), serde_json::json!(status));
    r
}

/// Build a record with `id`, `name`, and `category` fields.
pub fn record_with_category(id: &str, name: &str, category: &str) -> Record {
    let mut r = record(id, name);
    r.insert("category".into(), serde_json::json!(category));
    r
}

/// Build a record with `id`, `name`, `price` (numeric), and optionally `category`.
pub fn product(id: &str, name: &str, price: f64) -> Record {
    let mut r = record(id, name);
    r.insert("price".into(), serde_json::json!(price));
    r
}

/// Build a product record with category.
pub fn product_with_category(id: &str, name: &str, price: f64, category: &str) -> Record {
    let mut r = product(id, name, price);
    r.insert("category".into(), serde_json::json!(category));
    r
}

/// Build a numbered record for load/stress tests.
///
/// Fields: `id` (string of index), `name` (formatted), `payload`.
pub fn numbered_record(index: usize, payload: &str) -> Record {
    let mut r = serde_json::Map::new();
    r.insert("id".into(), serde_json::json!(index.to_string()));
    r.insert("name".into(), serde_json::json!(format!("item-{index}")));
    r.insert("payload".into(), serde_json::json!(payload));
    r
}

/// Build a record with `id`, `value`, and optional extra fields via closure.
pub fn record_with<F>(id: &str, f: F) -> Record
where
    F: FnOnce(&mut Record),
{
    let mut r = serde_json::Map::new();
    r.insert("id".into(), serde_json::json!(id));
    f(&mut r);
    r
}

// =============================================================================
// Notifications
// =============================================================================

/// Build a console notification with the given recipient and body.
pub fn notification(recipient: &str, body: &str) -> Notification {
    Notification {
        id: uuid::Uuid::new_v4().to_string(),
        channel: NotificationChannel::Console,
        recipient: recipient.into(),
        subject: None,
        body: body.into(),
        html_body: None,
        template_id: None,
        template_vars: Default::default(),
        metadata: Default::default(),
        priority: Default::default(),
        scheduled_at: None,
    }
}

/// Build an email notification with subject.
pub fn email_notification(recipient: &str, subject: &str, body: &str) -> Notification {
    let mut n = notification(recipient, body);
    n.channel = NotificationChannel::Email;
    n.subject = Some(subject.into());
    n
}

// =============================================================================
// Payment Types
// =============================================================================

/// Create a Money value in USD cents.
pub fn usd(cents: i64) -> Money {
    Money::new(cents, Currency::Usd)
}

/// Create a test customer.
pub fn customer(id: &str, name: &str, email: &str) -> Customer {
    Customer {
        id: id.into(),
        email: Some(email.into()),
        name: Some(name.into()),
        phone: None,
        metadata: Default::default(),
        created_at: chrono::Utc::now(),
    }
}
