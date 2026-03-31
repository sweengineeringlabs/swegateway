//! End-to-end tests for NotificationOutbound sub-trait.
//!
//! Exercises only the NotificationOutbound write operations through the SAF factory.
//! Results are verified using NotificationInbound (get_status, list_sent) on the
//! same combined gateway instance.

use swe_gateway::prelude::*;
use swe_gateway::saf::notification::{Notification, NotificationChannel, NotificationStatus};
use swe_gateway::saf;

#[tokio::test]
async fn e2e_notification_outbound_send_single_and_verify() {
    // impl NotificationGateway implements both NotificationInbound and NotificationOutbound
    let notifier = saf::silent_notifier();

    // --- NotificationOutbound: send ---
    let notification = Notification::new(
        NotificationChannel::Console,
        "customer@example.com",
        "Your order has been shipped!",
    )
    .with_subject("Order Shipped");
    let notif_id = notification.id.clone();

    let receipt = notifier.send(notification).await.unwrap();

    assert_eq!(receipt.notification_id, notif_id);
    assert_eq!(receipt.status, NotificationStatus::Delivered);
    assert!(receipt.provider_id.is_some(), "Silent notifier should assign a provider ID");

    // Verify via NotificationInbound
    let status = notifier.get_status(&notif_id).await.unwrap();
    assert_eq!(status.notification_id, notif_id);
    assert_eq!(status.status, NotificationStatus::Delivered);
}

#[tokio::test]
async fn e2e_notification_outbound_send_batch_multi_channel() {
    let notifier = saf::silent_notifier();

    // --- NotificationOutbound: send_batch with mixed channels ---
    let notifications = vec![
        Notification::new(NotificationChannel::Email, "a@example.com", "Email body").with_subject("Email Subject"),
        Notification::new(NotificationChannel::Console, "b@example.com", "Console body"),
        Notification::new(NotificationChannel::Sms, "+1555000001", "SMS body"),
        Notification::new(NotificationChannel::Push, "device-token-xyz", "Push body").with_subject("Push Title"),
        Notification::new(NotificationChannel::Console, "c@example.com", "Another console"),
    ];

    let ids: Vec<String> = notifications.iter().map(|n| n.id.clone()).collect();

    let receipts = notifier.send_batch(notifications).await.unwrap();

    assert_eq!(receipts.len(), 5);
    for receipt in &receipts {
        assert_eq!(
            receipt.status,
            NotificationStatus::Delivered,
            "All batch notifications should be delivered: {:?}",
            receipt
        );
    }

    // Verify via NotificationInbound: all IDs should be retrievable
    let all_sent = notifier.list_sent(10, 0).await.unwrap();
    assert_eq!(all_sent.len(), 5);

    for id in &ids {
        let status = notifier.get_status(id).await.unwrap();
        assert_eq!(status.status, NotificationStatus::Delivered);
    }
}

#[tokio::test]
async fn e2e_notification_outbound_cancel_delivered_returns_false() {
    let notifier = saf::silent_notifier();

    // Send a notification (it is delivered immediately in the silent notifier)
    let n = Notification::new(
        NotificationChannel::Console,
        "user@example.com",
        "Immediate delivery notification",
    );
    let id = n.id.clone();
    let receipt = notifier.send(n).await.unwrap();
    assert_eq!(receipt.status, NotificationStatus::Delivered);

    // --- NotificationOutbound: cancel ---
    // Already-delivered notifications cannot be cancelled, so cancel returns false
    let cancelled = notifier.cancel(&id).await.unwrap();
    assert!(
        !cancelled,
        "Cancelling an already-delivered notification should return false"
    );

    // Verify the notification is still recorded as Delivered
    let status = notifier.get_status(&id).await.unwrap();
    assert_eq!(status.status, NotificationStatus::Delivered);
}
