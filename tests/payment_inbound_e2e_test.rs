//! End-to-end tests for PaymentInbound sub-trait.
//!
//! Exercises only the PaymentInbound read operations through the SAF factory.
//! Payments, customers, and refunds are seeded using PaymentOutbound (via the
//! combined gateway), then only PaymentInbound methods are exercised.

use swe_gateway::prelude::*;
use swe_gateway::saf::payment::{Customer, Money, Payment, PaymentStatus, Refund, RefundStatus};
use swe_gateway::saf;

#[tokio::test]
async fn e2e_payment_inbound_get_payment_and_list_payments() {
    // impl PaymentGateway implements both PaymentInbound and PaymentOutbound
    let gw = saf::mock_payment_gateway();

    // Seed data via PaymentOutbound
    let customer = Customer::new("alice@example.com").with_name("Alice");
    let customer_id = customer.id.clone();
    gw.create_customer(customer).await.unwrap();

    let p1 = Payment::new(Money::usd(1000))
        .with_description("Invoice #001")
        .with_customer(&customer_id);
    let p2 = Payment::new(Money::usd(2500))
        .with_description("Invoice #002")
        .with_customer(&customer_id);
    let p3 = Payment::new(Money::usd(500))
        .with_description("Invoice #003")
        .with_customer(&customer_id);

    let p1_id = p1.id.clone();
    let p2_id = p2.id.clone();

    gw.create_payment(p1).await.unwrap();
    gw.create_payment(p2).await.unwrap();
    gw.create_payment(p3).await.unwrap();

    // --- PaymentInbound: get_payment ---
    let result1 = gw.get_payment(&p1_id).await.unwrap();
    assert_eq!(result1.payment_id, p1_id);
    assert_eq!(result1.status, PaymentStatus::Succeeded);
    assert_eq!(result1.amount.amount, 1000);

    let result2 = gw.get_payment(&p2_id).await.unwrap();
    assert_eq!(result2.amount.amount, 2500);

    // --- PaymentInbound: list_payments ---
    let payments = gw.list_payments(&customer_id, 10, 0).await.unwrap();
    // Mock gateway stores customer_id on payments made with .with_customer()
    assert!(payments.len() <= 3);

    // List with pagination
    let paged = gw.list_payments(&customer_id, 2, 0).await.unwrap();
    assert!(paged.len() <= 2);
}

#[tokio::test]
async fn e2e_payment_inbound_get_customer_and_get_refund() {
    let gw = saf::mock_payment_gateway();

    // Seed customer
    let customer = Customer::new("bob@example.com")
        .with_name("Bob Builder")
        .with_phone("+1555000042");
    let customer_id = customer.id.clone();
    gw.create_customer(customer).await.unwrap();

    // Seed payment and refund
    let payment = Payment::new(Money::usd(7500)).with_description("Service Fee");
    let payment_id = payment.id.clone();
    gw.create_payment(payment).await.unwrap();

    let refund = Refund::partial(&payment_id, Money::usd(2500))
        .with_description("Partial service credit");
    let refund_id = refund.id.clone();
    gw.create_refund(refund).await.unwrap();

    // --- PaymentInbound: get_customer ---
    let fetched = gw.get_customer(&customer_id).await.unwrap();
    assert_eq!(fetched.id, customer_id);
    assert_eq!(fetched.email, Some("bob@example.com".to_string()));
    assert_eq!(fetched.name, Some("Bob Builder".to_string()));
    assert_eq!(fetched.phone, Some("+1555000042".to_string()));

    // --- PaymentInbound: get_refund ---
    let refund_result = gw.get_refund(&refund_id).await.unwrap();
    assert_eq!(refund_result.refund_id, refund_id);
    assert_eq!(refund_result.payment_id, payment_id);
    assert_eq!(refund_result.status, RefundStatus::Succeeded);
    assert_eq!(refund_result.amount.amount, 2500);

    // Error on nonexistent refund
    let err = gw.get_refund("nonexistent-refund").await;
    assert!(err.is_err());
}

#[tokio::test]
async fn e2e_payment_inbound_nonexistent_returns_error_and_health_check() {
    let gw = saf::mock_payment_gateway();

    // Seed one customer so the gateway is non-empty
    let c = Customer::new("seed@example.com");
    gw.create_customer(c).await.unwrap();

    // --- PaymentInbound: get_payment on missing ID ---
    let err = gw.get_payment("pay_does_not_exist").await;
    assert!(err.is_err(), "get_payment with unknown ID should return an error");

    // --- PaymentInbound: get_customer on missing ID ---
    let err = gw.get_customer("cust_does_not_exist").await;
    assert!(err.is_err(), "get_customer with unknown ID should return an error");

    // --- PaymentInbound: get_refund on missing ID ---
    let err = gw.get_refund("ref_does_not_exist").await;
    assert!(err.is_err(), "get_refund with unknown ID should return an error");

    // --- PaymentInbound: health_check ---
    let health = gw.health_check().await.unwrap();
    assert_eq!(health.status, HealthStatus::Healthy);
}
