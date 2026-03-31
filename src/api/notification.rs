//! Notification gateway types and configuration.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Notification channel type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationChannel {
    /// Email notification.
    Email,
    /// SMS text message.
    Sms,
    /// Push notification.
    Push,
    /// In-app notification.
    InApp,
    /// Webhook notification.
    Webhook,
    /// Console output (for development/testing).
    Console,
}

impl Default for NotificationChannel {
    fn default() -> Self {
        Self::Console
    }
}

/// Configuration for notifications.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NotificationConfig {
    /// Default channel.
    pub default_channel: NotificationChannel,
    /// Email configuration.
    pub email: Option<EmailConfig>,
    /// SMS configuration.
    pub sms: Option<SmsConfig>,
    /// Push notification configuration.
    pub push: Option<PushConfig>,
    /// Webhook configuration.
    pub webhook: Option<WebhookConfig>,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            default_channel: NotificationChannel::Console,
            email: None,
            sms: None,
            push: None,
            webhook: None,
        }
    }
}

/// Email-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    /// SMTP host.
    pub smtp_host: String,
    /// SMTP port.
    pub smtp_port: u16,
    /// Username for SMTP auth.
    pub username: Option<String>,
    /// Password for SMTP auth.
    #[serde(skip_serializing)]
    pub password: Option<String>,
    /// Default sender email.
    pub from_email: String,
    /// Default sender name.
    pub from_name: Option<String>,
    /// Use TLS.
    pub use_tls: bool,
}

/// SMS-specific configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SmsConfig {
    /// Provider (e.g., "twilio", "nexmo").
    pub provider: String,
    /// Account SID or API key.
    #[serde(skip_serializing)]
    pub account_id: String,
    /// Auth token or API secret.
    #[serde(skip_serializing)]
    pub auth_token: String,
    /// Default sender phone number.
    pub from_number: String,
}

/// Push notification configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushConfig {
    /// Provider (e.g., "firebase", "apns").
    pub provider: String,
    /// API key or credentials.
    #[serde(skip_serializing)]
    pub api_key: String,
    /// Additional options.
    #[serde(default)]
    pub options: HashMap<String, String>,
}

/// Webhook configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    /// Default webhook URL.
    pub url: String,
    /// Authentication header.
    pub auth_header: Option<String>,
    /// Authentication value.
    #[serde(skip_serializing)]
    pub auth_value: Option<String>,
}

/// A notification to be sent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    /// Unique notification ID.
    pub id: String,
    /// Notification channel.
    pub channel: NotificationChannel,
    /// Recipient address (email, phone, device token, etc.).
    pub recipient: String,
    /// Notification subject (for email) or title (for push).
    pub subject: Option<String>,
    /// Notification body/message.
    pub body: String,
    /// HTML body (for email).
    pub html_body: Option<String>,
    /// Template ID (if using templates).
    pub template_id: Option<String>,
    /// Template variables.
    #[serde(default)]
    pub template_vars: HashMap<String, serde_json::Value>,
    /// Additional metadata.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    /// Priority level.
    pub priority: NotificationPriority,
    /// Scheduled send time.
    pub scheduled_at: Option<DateTime<Utc>>,
}

impl Notification {
    /// Creates a new notification.
    pub fn new(
        channel: NotificationChannel,
        recipient: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            channel,
            recipient: recipient.into(),
            subject: None,
            body: body.into(),
            html_body: None,
            template_id: None,
            template_vars: HashMap::new(),
            metadata: HashMap::new(),
            priority: NotificationPriority::Normal,
            scheduled_at: None,
        }
    }

    /// Creates an email notification.
    pub fn email(recipient: impl Into<String>, subject: impl Into<String>, body: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            channel: NotificationChannel::Email,
            recipient: recipient.into(),
            subject: Some(subject.into()),
            body: body.into(),
            html_body: None,
            template_id: None,
            template_vars: HashMap::new(),
            metadata: HashMap::new(),
            priority: NotificationPriority::Normal,
            scheduled_at: None,
        }
    }

    /// Creates an SMS notification.
    pub fn sms(recipient: impl Into<String>, body: impl Into<String>) -> Self {
        Self::new(NotificationChannel::Sms, recipient, body)
    }

    /// Creates a push notification.
    pub fn push(recipient: impl Into<String>, title: impl Into<String>, body: impl Into<String>) -> Self {
        let mut n = Self::new(NotificationChannel::Push, recipient, body);
        n.subject = Some(title.into());
        n
    }

    /// Creates a console notification (for testing).
    pub fn console(body: impl Into<String>) -> Self {
        Self::new(NotificationChannel::Console, "console", body)
    }

    /// Sets the subject/title.
    pub fn with_subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = Some(subject.into());
        self
    }

    /// Sets the HTML body.
    pub fn with_html(mut self, html: impl Into<String>) -> Self {
        self.html_body = Some(html.into());
        self
    }

    /// Sets a template.
    pub fn with_template(mut self, template_id: impl Into<String>) -> Self {
        self.template_id = Some(template_id.into());
        self
    }

    /// Adds a template variable.
    pub fn with_var(mut self, key: impl Into<String>, value: impl Into<serde_json::Value>) -> Self {
        self.template_vars.insert(key.into(), value.into());
        self
    }

    /// Sets the priority.
    pub fn with_priority(mut self, priority: NotificationPriority) -> Self {
        self.priority = priority;
        self
    }

    /// Schedules the notification.
    pub fn scheduled_at(mut self, time: DateTime<Utc>) -> Self {
        self.scheduled_at = Some(time);
        self
    }
}

/// Notification priority level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationPriority {
    /// Low priority.
    Low,
    /// Normal priority.
    #[default]
    Normal,
    /// High priority.
    High,
    /// Critical/urgent priority.
    Critical,
}

/// Receipt/acknowledgment of a sent notification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationReceipt {
    /// Notification ID.
    pub notification_id: String,
    /// Provider message ID (if available).
    pub provider_id: Option<String>,
    /// Delivery status.
    pub status: NotificationStatus,
    /// Status message.
    pub message: Option<String>,
    /// Timestamp of the status update.
    pub timestamp: DateTime<Utc>,
    /// Additional metadata from the provider.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl NotificationReceipt {
    /// Creates a successful receipt.
    pub fn success(notification_id: impl Into<String>) -> Self {
        Self {
            notification_id: notification_id.into(),
            provider_id: None,
            status: NotificationStatus::Sent,
            message: None,
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Creates a failed receipt.
    pub fn failed(notification_id: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            notification_id: notification_id.into(),
            provider_id: None,
            status: NotificationStatus::Failed,
            message: Some(message.into()),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Sets the provider ID.
    pub fn with_provider_id(mut self, id: impl Into<String>) -> Self {
        self.provider_id = Some(id.into());
        self
    }
}

/// Notification delivery status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NotificationStatus {
    /// Notification is pending/queued.
    Pending,
    /// Notification was sent.
    Sent,
    /// Notification was delivered.
    Delivered,
    /// Notification was opened/read.
    Opened,
    /// Notification failed.
    Failed,
    /// Notification was bounced.
    Bounced,
    /// Recipient complained (spam).
    Complained,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_new() {
        let n = Notification::new(NotificationChannel::Email, "user@example.com", "Hello");
        assert_eq!(n.channel, NotificationChannel::Email);
        assert_eq!(n.recipient, "user@example.com");
        assert_eq!(n.body, "Hello");
        assert_eq!(n.priority, NotificationPriority::Normal);
        assert!(!n.id.is_empty());
    }

    /// @covers: email
    #[test]
    fn test_notification_email_with_subject() {
        let n = Notification::email("user@test.com", "Test Subject", "Body text");
        assert_eq!(n.channel, NotificationChannel::Email);
        assert_eq!(n.subject, Some("Test Subject".to_string()));
        assert_eq!(n.body, "Body text");
    }

    /// @covers: console
    #[test]
    fn test_notification_console() {
        let n = Notification::console("debug message");
        assert_eq!(n.channel, NotificationChannel::Console);
        assert_eq!(n.recipient, "console");
    }

    /// @covers: success
    #[test]
    fn test_notification_receipt_success() {
        let r = NotificationReceipt::success("notif-1");
        assert_eq!(r.notification_id, "notif-1");
        assert_eq!(r.status, NotificationStatus::Sent);
        assert!(r.message.is_none());
    }

    /// @covers: failed
    #[test]
    fn test_notification_receipt_failed() {
        let r = NotificationReceipt::failed("notif-2", "delivery error");
        assert_eq!(r.status, NotificationStatus::Failed);
        assert_eq!(r.message, Some("delivery error".to_string()));
    }

    /// @covers: sms
    #[test]
    fn test_sms() {
        let n = Notification::sms("+1234567890", "hello");
        assert_eq!(n.channel, NotificationChannel::Sms);
        assert_eq!(n.recipient, "+1234567890");
        assert_eq!(n.body, "hello");
    }

    /// @covers: push
    #[test]
    fn test_push() {
        let n = Notification::push("device-tok", "Title", "Body");
        assert_eq!(n.channel, NotificationChannel::Push);
        assert_eq!(n.recipient, "device-tok");
        assert_eq!(n.subject, Some("Title".to_string()));
        assert_eq!(n.body, "Body");
    }

    /// @covers: with_html
    #[test]
    fn test_with_html() {
        let n = Notification::email("x", "s", "b").with_html("<b>hi</b>");
        assert_eq!(n.html_body, Some("<b>hi</b>".to_string()));
    }

    /// @covers: with_template
    #[test]
    fn test_with_template() {
        let n = Notification::console("x").with_template("tmpl-1");
        assert_eq!(n.template_id, Some("tmpl-1".to_string()));
    }

    /// @covers: with_var
    #[test]
    fn test_with_var() {
        let n = Notification::console("x").with_var("name", "Alice");
        assert_eq!(
            n.template_vars.get("name"),
            Some(&serde_json::Value::String("Alice".to_string()))
        );
    }

    /// @covers: with_priority
    #[test]
    fn test_with_priority() {
        let n = Notification::console("x").with_priority(NotificationPriority::High);
        assert_eq!(n.priority, NotificationPriority::High);
    }

    /// @covers: scheduled_at
    #[test]
    fn test_scheduled_at() {
        let now = Utc::now();
        let n = Notification::console("x").scheduled_at(now);
        assert_eq!(n.scheduled_at, Some(now));
    }

    /// @covers: with_provider_id
    #[test]
    fn test_with_provider_id() {
        let r = NotificationReceipt::success("n1").with_provider_id("p1");
        assert_eq!(r.provider_id, Some("p1".to_string()));
    }

    /// @covers: with_subject
    #[test]
    fn test_with_subject() {
        let n = Notification::console("body").with_subject("My Subject");
        assert_eq!(n.subject, Some("My Subject".to_string()));
    }
}
