// @allow: no_mocks_in_integration — mocks external LLM API boundary
//! Integration tests for the gateway abstractions.

use swe_gateway::prelude::*;
use swe_gateway::saf;
use swe_gateway::saf::{
    database::QueryParams,
    file::UploadOptions,
    notification::Notification,
    payment::{Money, Payment, Refund},
};

// =============================================================================
// Database Gateway Tests
// =============================================================================

mod database_tests {
    use super::*;

    #[tokio::test]
    async fn test_memory_database_crud() {
        let db = saf::memory_database();

        // Insert
        let mut record = serde_json::Map::new();
        record.insert("id".to_string(), serde_json::json!("user-1"));
        record.insert("name".to_string(), serde_json::json!("Alice"));
        record.insert("email".to_string(), serde_json::json!("alice@example.com"));

        let result = db.insert("users", record).await.unwrap();
        assert_eq!(result.inserted_id, Some("user-1".to_string()));

        // Read
        let retrieved = db.get_by_id("users", "user-1").await.unwrap();
        assert!(retrieved.is_some());
        let user = retrieved.unwrap();
        assert_eq!(user.get("name").unwrap(), "Alice");

        // Update
        let mut updates = serde_json::Map::new();
        updates.insert("name".to_string(), serde_json::json!("Alice Smith"));
        db.update("users", "user-1", updates).await.unwrap();

        let updated = db.get_by_id("users", "user-1").await.unwrap().unwrap();
        assert_eq!(updated.get("name").unwrap(), "Alice Smith");

        // Delete
        db.delete("users", "user-1").await.unwrap();
        assert!(!db.exists("users", "user-1").await.unwrap());
    }

    #[tokio::test]
    async fn test_query_with_filters() {
        let db = saf::memory_database();

        // Insert multiple records
        for i in 1..=10 {
            let mut record = serde_json::Map::new();
            record.insert("id".to_string(), serde_json::json!(format!("item-{}", i)));
            record.insert("category".to_string(), serde_json::json!(if i % 2 == 0 { "even" } else { "odd" }));
            record.insert("value".to_string(), serde_json::json!(i * 10));
            db.insert("items", record).await.unwrap();
        }

        // Query with filter
        let params = QueryParams::new().filter("category", "even");
        let results = db.query("items", params).await.unwrap();
        assert_eq!(results.len(), 5);

        // Count
        let count = db.count("items", QueryParams::new()).await.unwrap();
        assert_eq!(count, 10);
    }

    #[tokio::test]
    async fn test_batch_operations() {
        let db = saf::memory_database();

        // Batch insert
        let records: Vec<_> = (1..=5)
            .map(|i| {
                let mut record = serde_json::Map::new();
                record.insert("id".to_string(), serde_json::json!(format!("batch-{}", i)));
                record.insert("status".to_string(), serde_json::json!("pending"));
                record
            })
            .collect();

        let result = db.batch_insert("tasks", records).await.unwrap();
        assert_eq!(result.rows_affected, 5);

        // Update where
        let mut updates = serde_json::Map::new();
        updates.insert("status".to_string(), serde_json::json!("completed"));

        db.update_where("tasks", QueryParams::new(), updates)
            .await
            .unwrap();

        // Verify all updated
        let results = db
            .query("tasks", QueryParams::new().filter("status", "completed"))
            .await
            .unwrap();
        assert_eq!(results.len(), 5);
    }

    #[tokio::test]
    async fn test_health_check() {
        let db = saf::memory_database();
        let health = db.health_check().await.unwrap();
        assert_eq!(health.status, HealthStatus::Healthy);
    }
}

// =============================================================================
// File Gateway Tests
// =============================================================================

mod file_tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_local_file_operations() {
        let temp_dir = TempDir::new().unwrap();
        let gateway = saf::local_file_gateway(temp_dir.path());

        // Write
        let content = b"Hello, World!".to_vec();
        let info = gateway
            .write("test.txt", content.clone(), UploadOptions::overwrite())
            .await
            .unwrap();
        assert_eq!(info.size, 13);

        // Exists
        assert!(gateway.exists("test.txt").await.unwrap());

        // Read
        let read_content = gateway.read("test.txt").await.unwrap();
        assert_eq!(read_content, content);

        // Metadata
        let metadata = gateway.metadata("test.txt").await.unwrap();
        assert_eq!(metadata.size, 13);
        assert!(!metadata.is_directory);

        // Copy
        gateway.copy("test.txt", "copy.txt").await.unwrap();
        assert!(gateway.exists("copy.txt").await.unwrap());

        // Rename
        gateway.rename("copy.txt", "renamed.txt").await.unwrap();
        assert!(gateway.exists("renamed.txt").await.unwrap());
        assert!(!gateway.exists("copy.txt").await.unwrap());

        // Delete
        gateway.delete("test.txt").await.unwrap();
        assert!(!gateway.exists("test.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_directory_operations() {
        let temp_dir = TempDir::new().unwrap();
        let gateway = saf::local_file_gateway(temp_dir.path());

        // Create directory
        gateway.create_directory("subdir").await.unwrap();

        // Write file in subdirectory
        gateway
            .write(
                "subdir/file.txt",
                b"nested content".to_vec(),
                UploadOptions::overwrite(),
            )
            .await
            .unwrap();

        assert!(gateway.exists("subdir/file.txt").await.unwrap());

        // Delete directory (recursive)
        gateway.delete_directory("subdir", true).await.unwrap();
        assert!(!gateway.exists("subdir/file.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_list_files() {
        let temp_dir = TempDir::new().unwrap();
        let gateway = saf::local_file_gateway(temp_dir.path());

        // Create some files
        for i in 1..=3 {
            gateway
                .write(
                    &format!("file{}.txt", i),
                    format!("content {}", i).into_bytes(),
                    UploadOptions::overwrite(),
                )
                .await
                .unwrap();
        }

        // List all files
        let result = gateway
            .list(swe_gateway::saf::file::ListOptions::default())
            .await
            .unwrap();
        assert_eq!(result.files.len(), 3);
    }
}

// =============================================================================
// HTTP Gateway Tests
// =============================================================================

mod http_tests {
    use super::*;
    use swe_gateway::saf::http::HttpRequest;

    #[tokio::test]
    async fn test_rest_client_mock() {
        let client = saf::rest_client_with_base_url("https://api.example.com");

        // GET request
        let response = client.get("/users").await.unwrap();
        assert!(response.is_success());
        assert_eq!(response.status, 200);

        // POST request
        let body = serde_json::json!({"name": "test"});
        let response = client.post_json("/users", body).await.unwrap();
        assert!(response.is_success());
    }

    #[tokio::test]
    async fn test_http_request_builder() {
        let request = HttpRequest::get("/users")
            .with_header("Accept", "application/json")
            .with_query("page", "1")
            .with_query("limit", "10");

        assert_eq!(request.headers.get("Accept"), Some(&"application/json".to_string()));
        assert_eq!(request.query.get("page"), Some(&"1".to_string()));
    }

    #[tokio::test]
    async fn test_health_check() {
        let client = saf::rest_client_with_base_url("https://api.example.com");
        let health = client.health_check().await.unwrap();
        assert_eq!(health.status, HealthStatus::Healthy);
    }
}

// =============================================================================
// Notification Gateway Tests
// =============================================================================

mod notification_tests {
    use super::*;
    use swe_gateway::saf::notification::{NotificationChannel, NotificationStatus};

    #[tokio::test]
    async fn test_console_notifier() {
        let notifier = saf::silent_notifier();

        // Send notification
        let notification = Notification::new(
            NotificationChannel::Console,
            "user@example.com",
            "Test message body",
        )
        .with_subject("Test Subject");

        let receipt = notifier.send(notification.clone()).await.unwrap();
        assert_eq!(receipt.notification_id, notification.id);
        assert_eq!(receipt.status, NotificationStatus::Delivered);

        // Check status
        let status = notifier.get_status(&notification.id).await.unwrap();
        assert_eq!(status.status, NotificationStatus::Delivered);
    }

    #[tokio::test]
    async fn test_batch_notifications() {
        let notifier = saf::silent_notifier();

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

    #[tokio::test]
    async fn test_list_sent() {
        let notifier = saf::silent_notifier();

        // Send some notifications
        for i in 1..=5 {
            notifier
                .send(Notification::console(format!("Message {}", i)))
                .await
                .unwrap();
        }

        // List sent
        let sent = notifier.list_sent(3, 0).await.unwrap();
        assert_eq!(sent.len(), 3);
    }
}

// =============================================================================
// Payment Gateway Tests
// =============================================================================

mod payment_tests {
    use super::*;
    use swe_gateway::saf::payment::{Customer, PaymentStatus, RefundReason, RefundStatus};
    use swe_gateway::saf::MockFailureMode;

    #[tokio::test]
    async fn test_payment_flow() {
        let gateway = saf::mock_payment_gateway();

        // Create customer
        let customer = Customer::new("customer@example.com").with_name("John Doe");
        let created_customer = gateway.create_customer(customer.clone()).await.unwrap();
        assert_eq!(created_customer.email, Some("customer@example.com".to_string()));

        // Create payment
        let payment = Payment::new(Money::usd(2500))
            .with_customer(&created_customer.id)
            .with_description("Test purchase");

        let result = gateway.create_payment(payment.clone()).await.unwrap();
        assert_eq!(result.status, PaymentStatus::Succeeded);
        assert_eq!(result.amount.amount, 2500);

        // Get payment
        let retrieved = gateway.get_payment(&payment.id).await.unwrap();
        assert_eq!(retrieved.status, PaymentStatus::Succeeded);
    }

    #[tokio::test]
    async fn test_refund() {
        let gateway = saf::mock_payment_gateway();

        // Create payment
        let payment = Payment::new(Money::usd(5000));
        gateway.create_payment(payment.clone()).await.unwrap();

        // Full refund
        let refund = Refund::full(&payment.id).with_reason(RefundReason::CustomerRequest);
        let result = gateway.create_refund(refund).await.unwrap();
        assert_eq!(result.status, RefundStatus::Succeeded);
        assert_eq!(result.amount.amount, 5000);

        // Check payment status
        let payment_status = gateway.get_payment(&payment.id).await.unwrap();
        assert_eq!(payment_status.status, PaymentStatus::Refunded);
    }

    #[tokio::test]
    async fn test_partial_refund() {
        let gateway = saf::mock_payment_gateway();

        // Create payment
        let payment = Payment::new(Money::usd(10000));
        gateway.create_payment(payment.clone()).await.unwrap();

        // Partial refund
        let refund = Refund::partial(&payment.id, Money::usd(3000));
        let result = gateway.create_refund(refund).await.unwrap();
        assert_eq!(result.amount.amount, 3000);

        // Check payment status
        let payment_status = gateway.get_payment(&payment.id).await.unwrap();
        assert_eq!(payment_status.status, PaymentStatus::PartiallyRefunded);
    }

    #[tokio::test]
    async fn test_failure_mode() {
        let gateway = saf::mock_payment_gateway_with_failure(MockFailureMode::FailOverAmount(1000));

        // Small payment should succeed
        let small_payment = Payment::new(Money::usd(500));
        let result = gateway.create_payment(small_payment).await.unwrap();
        assert_eq!(result.status, PaymentStatus::Succeeded);

        // Large payment should fail
        let large_payment = Payment::new(Money::usd(2000));
        let result = gateway.create_payment(large_payment).await.unwrap();
        assert_eq!(result.status, PaymentStatus::Failed);
    }

    #[tokio::test]
    async fn test_customer_crud() {
        let gateway = saf::mock_payment_gateway();

        // Create
        let customer = Customer::new("test@example.com").with_name("Test User");
        gateway.create_customer(customer.clone()).await.unwrap();

        // Read
        let retrieved = gateway.get_customer(&customer.id).await.unwrap();
        assert_eq!(retrieved.name, Some("Test User".to_string()));

        // Update
        let mut updated = retrieved.clone();
        updated.name = Some("Updated Name".to_string());
        gateway.update_customer(updated).await.unwrap();

        let retrieved = gateway.get_customer(&customer.id).await.unwrap();
        assert_eq!(retrieved.name, Some("Updated Name".to_string()));

        // Delete
        gateway.delete_customer(&customer.id).await.unwrap();
        assert!(gateway.get_customer(&customer.id).await.is_err());
    }
}

// =============================================================================
// Cross-Gateway Integration Tests
// =============================================================================

mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_multiple_gateways_together() {
        // This test demonstrates using multiple gateways together
        // in a realistic scenario

        let db = saf::memory_database();
        let notifier = saf::silent_notifier();
        let payments = saf::mock_payment_gateway();

        // 1. Create a customer
        let customer = swe_gateway::saf::payment::Customer::new("order@example.com")
            .with_name("Order Customer");
        payments.create_customer(customer.clone()).await.unwrap();

        // 2. Store order in database
        let mut order = serde_json::Map::new();
        order.insert("id".to_string(), serde_json::json!("order-123"));
        order.insert("customer_id".to_string(), serde_json::json!(customer.id));
        order.insert("total".to_string(), serde_json::json!(9999));
        order.insert("status".to_string(), serde_json::json!("pending"));
        db.insert("orders", order).await.unwrap();

        // 3. Process payment
        let payment = Payment::new(Money::usd(9999)).with_customer(&customer.id);
        let payment_result = payments.create_payment(payment).await.unwrap();

        // 4. Update order status
        let mut updates = serde_json::Map::new();
        updates.insert("status".to_string(), serde_json::json!("paid"));
        updates.insert(
            "payment_id".to_string(),
            serde_json::json!(payment_result.payment_id),
        );
        db.update("orders", "order-123", updates).await.unwrap();

        // 5. Send confirmation notification
        let notification = Notification::email(
            "order@example.com",
            "Order Confirmed",
            "Your order #123 has been confirmed and paid.",
        );
        notifier.send(notification).await.unwrap();

        // Verify final state
        let order = db.get_by_id("orders", "order-123").await.unwrap().unwrap();
        assert_eq!(order.get("status").unwrap(), "paid");
    }
}
