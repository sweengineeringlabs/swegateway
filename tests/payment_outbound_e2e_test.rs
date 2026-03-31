//! End-to-end tests for PaymentOutbound sub-trait.
//!
//! Exercises only the PaymentOutbound write operations through the SAF factory.
//! Results are verified using PaymentInbound (get_payment, get_customer, get_refund)
//! on the same combined gateway instance.

use swe_gateway::prelude::*;
use swe_gateway::saf::payment::{Customer, Money, Payment, PaymentStatus, Refund, RefundStatus};
use swe_gateway::saf;

#[tokio::test]
async fn e2e_payment_outbound_create_capture_cancel_payment() {
    // impl PaymentGateway implements both PaymentInbound and PaymentOutbound
    let gw = saf::mock_payment_gateway();

    // --- PaymentOutbound: create_payment ---
    let payment = Payment::new(Money::usd(4000)).with_description("Subscription charge");
    let payment_id = payment.id.clone();
    let created = gw.create_payment(payment).await.unwrap();
    assert_eq!(created.payment_id, payment_id);
    assert_eq!(created.status, PaymentStatus::Succeeded);
    assert_eq!(created.amount.amount, 4000);

    // Verify via PaymentInbound
    let fetched = gw.get_payment(&payment_id).await.unwrap();
    assert_eq!(fetched.payment_id, payment_id);
    assert_eq!(fetched.status, PaymentStatus::Succeeded);

    // --- PaymentOutbound: capture_payment (with specific amount) ---
    let p2 = Payment::new(Money::usd(8000)).with_description("Auth for later capture");
    let p2_id = p2.id.clone();
    gw.create_payment(p2).await.unwrap();

    let captured = gw
        .capture_payment(&p2_id, Some(Money::usd(6000)))
        .await
        .unwrap();
    assert_eq!(captured.status, PaymentStatus::Succeeded);
    assert_eq!(captured.amount.amount, 6000);

    // --- PaymentOutbound: cancel_payment ---
    let p3 = Payment::new(Money::usd(1500)).with_description("Cancellable charge");
    let p3_id = p3.id.clone();
    gw.create_payment(p3).await.unwrap();

    let cancelled = gw.cancel_payment(&p3_id).await.unwrap();
    assert_eq!(cancelled.status, PaymentStatus::Canceled);

    // Verify cancellation persisted via PaymentInbound
    let after_cancel = gw.get_payment(&p3_id).await.unwrap();
    assert_eq!(after_cancel.status, PaymentStatus::Canceled);
}

#[tokio::test]
async fn e2e_payment_outbound_create_refund_full_and_partial() {
    let gw = saf::mock_payment_gateway();

    // Seed payment
    let payment = Payment::new(Money::usd(12000)).with_description("Annual subscription");
    let payment_id = payment.id.clone();
    gw.create_payment(payment).await.unwrap();

    // --- PaymentOutbound: create_refund (full) ---
    let full_refund = Refund::full(&payment_id).with_description("Customer cancellation");
    let refund_id = full_refund.id.clone();
    let refund_result = gw.create_refund(full_refund).await.unwrap();
    assert_eq!(refund_result.refund_id, refund_id);
    assert_eq!(refund_result.payment_id, payment_id);
    assert_eq!(refund_result.status, RefundStatus::Succeeded);
    assert_eq!(refund_result.amount.amount, 12000);

    // Payment should now be marked as Refunded
    let payment_status = gw.get_payment(&payment_id).await.unwrap();
    assert_eq!(payment_status.status, PaymentStatus::Refunded);

    // Partial refund flow
    let p2 = Payment::new(Money::usd(5000));
    let p2_id = p2.id.clone();
    gw.create_payment(p2).await.unwrap();

    // --- PaymentOutbound: create_refund (partial) ---
    let partial = Refund::partial(&p2_id, Money::usd(1500)).with_description("Partial credit");
    let partial_id = partial.id.clone();
    let partial_result = gw.create_refund(partial).await.unwrap();
    assert_eq!(partial_result.amount.amount, 1500);
    assert_eq!(partial_result.status, RefundStatus::Succeeded);

    // Verify via PaymentInbound
    let p2_status = gw.get_payment(&p2_id).await.unwrap();
    assert_eq!(p2_status.status, PaymentStatus::PartiallyRefunded);

    let refund_lookup = gw.get_refund(&partial_id).await.unwrap();
    assert_eq!(refund_lookup.amount.amount, 1500);
}

#[tokio::test]
async fn e2e_payment_outbound_customer_create_update_delete() {
    let gw = saf::mock_payment_gateway();

    // --- PaymentOutbound: create_customer ---
    let customer = Customer::new("carol@example.com")
        .with_name("Carol White")
        .with_phone("+1555001234");
    let customer_id = customer.id.clone();

    let created = gw.create_customer(customer).await.unwrap();
    assert_eq!(created.id, customer_id);
    assert_eq!(created.email, Some("carol@example.com".to_string()));
    assert_eq!(created.name, Some("Carol White".to_string()));

    // Verify via PaymentInbound
    let fetched = gw.get_customer(&customer_id).await.unwrap();
    assert_eq!(fetched.name, Some("Carol White".to_string()));

    // --- PaymentOutbound: update_customer ---
    let mut updated = fetched;
    updated.name = Some("Carol Green".to_string());
    updated.phone = Some("+1555009999".to_string());
    let update_result = gw.update_customer(updated).await.unwrap();
    assert_eq!(update_result.name, Some("Carol Green".to_string()));

    // Verify via PaymentInbound
    let after_update = gw.get_customer(&customer_id).await.unwrap();
    assert_eq!(after_update.name, Some("Carol Green".to_string()));
    assert_eq!(after_update.phone, Some("+1555009999".to_string()));

    // --- PaymentOutbound: delete_customer ---
    gw.delete_customer(&customer_id).await.unwrap();

    // Verify deletion via PaymentInbound
    let err = gw.get_customer(&customer_id).await;
    assert!(err.is_err(), "Deleted customer should not be retrievable");
}
