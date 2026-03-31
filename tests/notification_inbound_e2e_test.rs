//! End-to-end tests for NotificationInbound sub-trait.
//!
//! Exercises only the NotificationInbound read operations through the SAF factory.
//! Notifications are first sent using NotificationOutbound (via the combined gateway),
//! then only NotificationInbound methods are exercised.

use swe_gateway::prelude::*;
use swe_gateway::saf::notification::{Notification, NotificationChannel, NotificationStatus};
use swe_gateway::saf;

#[tokio::test]
async fn e2e_notification_inbound_get_status_after_send() {
    // impl NotificationGateway implements both NotificationInbound and NotificationOutbound
    let notifier = saf::silent_notifier();

    // Seed data via NotificationOutbound
    let n1 = Notification::new(
        NotificationChannel::Console,
        "user@example.com",
        "Your account has been activated.",
    )
    .with_subject("Account Activated");

    let n2 = Notification::new(
        NotificationChannel::Email,
        "admin@example.com",
        "System maintenance scheduled for Sunday.",
    )
    .with_subject("Maintenance Window");

    let id1 = n1.id.clone();
    let id2 = n2.id.clone();

    notifier.send(n1).await.unwrap();
    notifier.send(n2).await.unwrap();

    // --- NotificationInbound: get_status ---
    let status1 = notifier.get_status(&id1).await.unwrap();
    assert_eq!(status1.notification_id, id1);
    assert_eq!(status1.status, NotificationStatus::Delivered);

    let status2 = notifier.get_status(&id2).await.unwrap();
    assert_eq!(status2.notification_id, id2);
    assert_eq!(status2.status, NotificationStatus::Delivered);
}

#[tokio::test]
async fn e2e_notification_inbound_list_sent_and_pagination() {
    let notifier = saf::silent_notifier();

    // Seed 7 notifications via NotificationOutbound
    let notifications: Vec<Notification> = (1..=7)
        .map(|i| {
            Notification::new(
                NotificationChannel::Console,
                &format!("recipient{}@example.com", i),
                &format!("Broadcast message #{}", i),
            )
        })
        .collect();

    let ids: Vec<String> = notifications.iter().map(|n| n.id.clone()).collect();
    notifier.send_batch(notifications).await.unwrap();

    // --- NotificationInbound: list_sent (all) ---
    let all_sent = notifier.list_sent(20, 0).await.unwrap();
    assert_eq!(all_sent.len(), 7);

    // --- NotificationInbound: list_sent (paginated) ---
    let page1 = notifier.list_sent(3, 0).await.unwrap();
    assert_eq!(page1.len(), 3);

    let page2 = notifier.list_sent(3, 3).await.unwrap();
    assert_eq!(page2.len(), 3);

    let page3 = notifier.list_sent(3, 6).await.unwrap();
    assert_eq!(page3.len(), 1);

    // Pages must not overlap
    let page1_ids: Vec<&str> = page1.iter().map(|r| r.notification_id.as_str()).collect();
    let page2_ids: Vec<&str> = page2.iter().map(|r| r.notification_id.as_str()).collect();
    assert!(page1_ids.iter().all(|id| !page2_ids.contains(id)));

    // Every seeded notification should be retrievable by ID
    for id in &ids {
        let receipt = notifier.get_status(id).await.unwrap();
        assert_eq!(receipt.status, NotificationStatus::Delivered);
    }
}

#[tokio::test]
async fn e2e_notification_inbound_get_status_unknown_and_health_check() {
    let notifier = saf::silent_notifier();

    // Send one real notification
    let n = Notification::console("Test message for health check flow");
    notifier.send(n).await.unwrap();

    // --- NotificationInbound: get_status for unknown ID ---
    let unknown_receipt = notifier.get_status("unknown-notification-xyz").await.unwrap();
    assert_eq!(unknown_receipt.notification_id, "unknown-notification-xyz");
    assert_eq!(
        unknown_receipt.status,
        NotificationStatus::Pending,
        "Unknown notification ID should return Pending status"
    );
    assert!(
        unknown_receipt.message.is_some(),
        "Unknown notification receipt should include an explanatory message"
    );

    // --- NotificationInbound: health_check ---
    let health = notifier.health_check().await.unwrap();
    assert_eq!(health.status, HealthStatus::Healthy);
}
