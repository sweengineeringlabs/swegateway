//! Console notification implementation for development and testing.

use chrono::Utc;
use futures::future::BoxFuture;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::api::{
    notification::{Notification, NotificationReceipt, NotificationStatus},
    traits::{NotificationGateway, NotificationInbound, NotificationOutbound},
    types::{GatewayResult, HealthCheck},
};

/// Console notification gateway that prints notifications to stdout.
#[derive(Debug, Default)]
pub(crate) struct ConsoleNotifier {
    /// Store sent notifications for status tracking.
    sent: Arc<RwLock<HashMap<String, NotificationReceipt>>>,
    /// Whether to print notifications to console.
    verbose: bool,
}

impl ConsoleNotifier {
    /// Creates a new console notifier.
    pub fn new() -> Self {
        Self {
            sent: Arc::new(RwLock::new(HashMap::new())),
            verbose: true,
        }
    }

    /// Creates a silent notifier that doesn't print to console.
    pub fn silent() -> Self {
        Self {
            sent: Arc::new(RwLock::new(HashMap::new())),
            verbose: false,
        }
    }

    /// Sets verbosity.
    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    /// Formats a notification for console output.
    fn format_notification(&self, notification: &Notification) -> String {
        let mut output = String::new();

        output.push_str(&format!(
            "\n┌─────────────────────────────────────────────────────────────┐\n"
        ));
        output.push_str(&format!(
            "│ {:^59} │\n",
            format!("{:?} Notification", notification.channel)
        ));
        output.push_str(&format!(
            "├─────────────────────────────────────────────────────────────┤\n"
        ));
        output.push_str(&format!("│ ID:        {:<48} │\n", notification.id));
        output.push_str(&format!(
            "│ To:        {:<48} │\n",
            truncate(&notification.recipient, 48)
        ));

        if let Some(subject) = &notification.subject {
            output.push_str(&format!(
                "│ Subject:   {:<48} │\n",
                truncate(subject, 48)
            ));
        }

        output.push_str(&format!(
            "├─────────────────────────────────────────────────────────────┤\n"
        ));

        // Word wrap the body
        for line in word_wrap(&notification.body, 59) {
            output.push_str(&format!("│ {:<59} │\n", line));
        }

        output.push_str(&format!(
            "└─────────────────────────────────────────────────────────────┘\n"
        ));

        output
    }
}

/// Truncates a string to max length with ellipsis.
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

/// Word wraps text to fit within max_width.
fn word_wrap(text: &str, max_width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current_line = String::new();

    for word in text.split_whitespace() {
        if current_line.is_empty() {
            current_line = word.to_string();
        } else if current_line.len() + 1 + word.len() <= max_width {
            current_line.push(' ');
            current_line.push_str(word);
        } else {
            lines.push(current_line);
            current_line = word.to_string();
        }
    }

    if !current_line.is_empty() {
        lines.push(current_line);
    }

    if lines.is_empty() {
        lines.push(String::new());
    }

    lines
}

impl NotificationInbound for ConsoleNotifier {
    fn get_status(&self, notification_id: &str) -> BoxFuture<'_, GatewayResult<NotificationReceipt>> {
        let id = notification_id.to_string();
        Box::pin(async move {
            let sent = self.sent.read().unwrap();
            match sent.get(&id) {
                Some(receipt) => Ok(receipt.clone()),
                None => Ok(NotificationReceipt {
                    notification_id: id,
                    provider_id: None,
                    status: NotificationStatus::Pending,
                    message: Some("Notification not found".to_string()),
                    timestamp: Utc::now(),
                    metadata: HashMap::new(),
                }),
            }
        })
    }

    fn list_sent(
        &self,
        limit: usize,
        offset: usize,
    ) -> BoxFuture<'_, GatewayResult<Vec<NotificationReceipt>>> {
        Box::pin(async move {
            let sent = self.sent.read().unwrap();
            let receipts: Vec<NotificationReceipt> = sent
                .values()
                .skip(offset)
                .take(limit)
                .cloned()
                .collect();
            Ok(receipts)
        })
    }

    fn health_check(&self) -> BoxFuture<'_, GatewayResult<HealthCheck>> {
        Box::pin(async move { Ok(HealthCheck::healthy()) })
    }
}

impl NotificationOutbound for ConsoleNotifier {
    fn send(&self, notification: Notification) -> BoxFuture<'_, GatewayResult<NotificationReceipt>> {
        let verbose = self.verbose;
        Box::pin(async move {
            // Print to console if verbose
            if verbose {
                let formatted = self.format_notification(&notification);
                println!("{}", formatted);
            }

            // Create receipt
            let receipt = NotificationReceipt {
                notification_id: notification.id.clone(),
                provider_id: Some(format!("console-{}", uuid::Uuid::new_v4())),
                status: NotificationStatus::Delivered,
                message: None,
                timestamp: Utc::now(),
                metadata: HashMap::new(),
            };

            // Store receipt
            {
                let mut sent = self.sent.write().unwrap();
                sent.insert(notification.id.clone(), receipt.clone());
            }

            Ok(receipt)
        })
    }

    fn send_batch(
        &self,
        notifications: Vec<Notification>,
    ) -> BoxFuture<'_, GatewayResult<Vec<NotificationReceipt>>> {
        Box::pin(async move {
            let mut receipts = Vec::with_capacity(notifications.len());
            for notification in notifications {
                let receipt = self.send(notification).await?;
                receipts.push(receipt);
            }
            Ok(receipts)
        })
    }

    fn cancel(&self, notification_id: &str) -> BoxFuture<'_, GatewayResult<bool>> {
        let id = notification_id.to_string();
        Box::pin(async move {
            let mut sent = self.sent.write().unwrap();
            if let Some(receipt) = sent.get_mut(&id) {
                if receipt.status == NotificationStatus::Pending {
                    receipt.status = NotificationStatus::Failed;
                    receipt.message = Some("Cancelled".to_string());
                    return Ok(true);
                }
            }
            Ok(false)
        })
    }
}

impl NotificationGateway for ConsoleNotifier {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::notification::NotificationChannel;

    #[tokio::test]
    async fn test_send_notification() {
        let notifier = ConsoleNotifier::silent();

        let notification = Notification::new(
            NotificationChannel::Console,
            "test@example.com",
            "Hello, World!",
        );

        let receipt = notifier.send(notification.clone()).await.unwrap();

        assert_eq!(receipt.notification_id, notification.id);
        assert_eq!(receipt.status, NotificationStatus::Delivered);
    }

    #[tokio::test]
    async fn test_get_status() {
        let notifier = ConsoleNotifier::silent();

        let notification = Notification::console("Test message");
        let id = notification.id.clone();

        notifier.send(notification).await.unwrap();

        let status = notifier.get_status(&id).await.unwrap();
        assert_eq!(status.status, NotificationStatus::Delivered);
    }

    #[tokio::test]
    async fn test_batch_send() {
        let notifier = ConsoleNotifier::silent();

        let notifications = vec![
            Notification::console("Message 1"),
            Notification::console("Message 2"),
            Notification::console("Message 3"),
        ];

        let receipts = notifier.send_batch(notifications).await.unwrap();

        assert_eq!(receipts.len(), 3);
        for receipt in receipts {
            assert_eq!(receipt.status, NotificationStatus::Delivered);
        }
    }

    #[test]
    fn test_word_wrap() {
        let text = "This is a long message that needs to be wrapped to fit within the specified width";
        let wrapped = word_wrap(text, 20);

        for line in &wrapped {
            assert!(line.len() <= 20);
        }
    }

    /// @covers: silent
    #[test]
    fn test_silent_creates_non_verbose() {
        let notifier = ConsoleNotifier::silent();
        assert!(
            !notifier.verbose,
            "silent() should create a notifier with verbose=false"
        );
        // Verify the sent store is initialized and empty
        let sent = notifier.sent.read().unwrap();
        assert!(sent.is_empty(), "silent notifier should start with no sent messages");
    }

    /// @covers: with_verbose
    #[test]
    fn test_with_verbose_toggles_flag() {
        let notifier = ConsoleNotifier::new().with_verbose(false);
        assert!(
            !notifier.verbose,
            "with_verbose(false) should set verbose=false"
        );

        let notifier = ConsoleNotifier::silent().with_verbose(true);
        assert!(
            notifier.verbose,
            "with_verbose(true) should set verbose=true"
        );
    }

    /// @covers: silent
    #[tokio::test]
    async fn test_silent() {
        let notifier = ConsoleNotifier::silent();

        // Send a notification and verify it is delivered (silent mode still delivers)
        let notification = Notification::console("silent test");
        let receipt = notifier.send(notification).await.unwrap();
        assert_eq!(
            receipt.status,
            NotificationStatus::Delivered,
            "silent notifier should still deliver notifications"
        );
        assert!(
            receipt.provider_id.is_some(),
            "receipt should have a provider_id"
        );
    }

    /// @covers: with_verbose
    #[tokio::test]
    async fn test_with_verbose() {
        let notifier = ConsoleNotifier::new().with_verbose(false);

        // Send a notification and verify it is delivered
        let notification = Notification::console("verbose-off test");
        let id = notification.id.clone();
        let receipt = notifier.send(notification).await.unwrap();
        assert_eq!(
            receipt.status,
            NotificationStatus::Delivered,
            "with_verbose(false) notifier should still deliver"
        );

        // Verify we can retrieve the status afterward
        let status = notifier.get_status(&id).await.unwrap();
        assert_eq!(status.status, NotificationStatus::Delivered);
    }
}
