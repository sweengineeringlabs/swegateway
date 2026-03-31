//! End-to-end tests for NotificationGateway.
//!
//! Exercises the combined NotificationGateway trait through full flows:
//! send -> check status -> send batch -> list sent -> cancel.

use swe_gateway::prelude::*;
use swe_gateway::saf::notification::{Notification, NotificationChannel, NotificationStatus};
use swe_gateway::saf;

#[tokio::test]
async fn e2e_notification_send_and_check_status() {
    let notifier = saf::silent_notifier();

    // Send a notification
    let notification = Notification::new(
        NotificationChannel::Console,
        "user@example.com",
        "Welcome to the platform!",
    )
    .with_subject("Welcome");

    let receipt = notifier.send(notification.clone()).await.unwrap();
    assert_eq!(receipt.notification_id, notification.id);
    assert_eq!(receipt.status, NotificationStatus::Delivered);
    assert!(receipt.provider_id.is_some());

    // Check status by ID
    let status = notifier.get_status(&notification.id).await.unwrap();
    assert_eq!(status.status, NotificationStatus::Delivered);
    assert_eq!(status.notification_id, notification.id);
}

#[tokio::test]
async fn e2e_notification_batch_send_and_list() {
    let notifier = saf::silent_notifier();

    // Send a batch of notifications
    let notifications: Vec<Notification> = (1..=5)
        .map(|i| {
            Notification::new(
                NotificationChannel::Console,
                &format!("user{}@example.com", i),
                &format!("Message #{}", i),
            )
        })
        .collect();

    let ids: Vec<String> = notifications.iter().map(|n| n.id.clone()).collect();

    let receipts = notifier.send_batch(notifications).await.unwrap();
    assert_eq!(receipts.len(), 5);
    for receipt in &receipts {
        assert_eq!(receipt.status, NotificationStatus::Delivered);
    }

    // List sent notifications
    let sent = notifier.list_sent(10, 0).await.unwrap();
    assert_eq!(sent.len(), 5);

    // Paginate — skip 2, take 2
    let page = notifier.list_sent(2, 2).await.unwrap();
    assert_eq!(page.len(), 2);

    // Each sent notification should be retrievable by ID
    for id in &ids {
        let status = notifier.get_status(id).await.unwrap();
        assert_eq!(status.status, NotificationStatus::Delivered);
    }
}

#[tokio::test]
async fn e2e_notification_unknown_id_returns_pending() {
    let notifier = saf::silent_notifier();

    // Query an unknown ID
    let status = notifier.get_status("unknown-id-123").await.unwrap();
    assert_eq!(status.status, NotificationStatus::Pending);
    assert!(status.message.is_some());
}

#[tokio::test]
async fn e2e_notification_cancel_not_applicable() {
    let notifier = saf::silent_notifier();

    // Send a notification (it gets delivered immediately in console mode)
    let notification = Notification::console("This will be delivered immediately");
    let id = notification.id.clone();
    notifier.send(notification).await.unwrap();

    // Cancel should return false because it's already delivered (not pending)
    let cancelled = notifier.cancel(&id).await.unwrap();
    assert!(!cancelled);
}

#[tokio::test]
async fn e2e_notification_health_check() {
    let notifier = saf::silent_notifier();

    let health = notifier.health_check().await.unwrap();
    assert_eq!(health.status, HealthStatus::Healthy);
}

#[tokio::test]
async fn e2e_notification_send_with_all_fields() {
    let notifier = saf::silent_notifier();

    let notification = Notification::new(
        NotificationChannel::Email,
        "admin@company.com",
        "System alert: disk usage at 95%",
    )
    .with_subject("ALERT: Disk Usage Critical");

    let receipt = notifier.send(notification).await.unwrap();
    assert_eq!(receipt.status, NotificationStatus::Delivered);
}
