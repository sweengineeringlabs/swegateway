//! Mock payment gateway implementation for testing.

use futures::future::BoxFuture;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::api::{
    payment::{
        Customer, Money, Payment, PaymentMethod, PaymentResult, PaymentStatus,
        Refund, RefundResult,
    },
    traits::{PaymentGateway, PaymentInbound, PaymentOutbound},
    types::{GatewayError, GatewayResult, HealthCheck},
};

/// Mock payment gateway for testing and development.
#[derive(Debug, Default)]
pub(crate) struct MockPaymentGateway {
    /// Stored payments.
    payments: Arc<RwLock<HashMap<String, PaymentResult>>>,
    /// Stored customers.
    customers: Arc<RwLock<HashMap<String, Customer>>>,
    /// Stored payment methods.
    payment_methods: Arc<RwLock<HashMap<String, Vec<PaymentMethod>>>>,
    /// Stored refunds.
    refunds: Arc<RwLock<HashMap<String, RefundResult>>>,
    /// Failure mode for testing error handling.
    failure_mode: Arc<RwLock<Option<MockFailureMode>>>,
}

pub(crate) use crate::api::types::MockFailureMode;

impl MockPaymentGateway {
    /// Creates a new mock payment gateway.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the failure mode for testing.
    pub fn with_failure_mode(self, mode: MockFailureMode) -> Self {
        *self.failure_mode.write().unwrap() = Some(mode);
        self
    }

    /// Clears the failure mode.
    pub fn clear_failure_mode(&self) {
        *self.failure_mode.write().unwrap() = None;
    }

    /// Checks if a payment should fail based on the failure mode.
    fn should_fail(&self, payment: &Payment) -> Option<String> {
        let mode = self.failure_mode.read().unwrap();
        match &*mode {
            Some(MockFailureMode::FailAllPayments(msg)) => Some(msg.clone()),
            Some(MockFailureMode::FailOverAmount(max)) => {
                if payment.amount.amount > *max {
                    Some(format!(
                        "Payment amount {} exceeds maximum {}",
                        payment.amount.amount, max
                    ))
                } else {
                    None
                }
            }
            Some(MockFailureMode::FailPaymentIds(ids)) => {
                if ids.contains(&payment.id) {
                    Some("Payment ID is in fail list".to_string())
                } else {
                    None
                }
            }
            None => None,
        }
    }

    /// Pre-seeds the gateway with a customer.
    pub fn add_customer(&self, customer: Customer) {
        let mut customers = self.customers.write().unwrap();
        customers.insert(customer.id.clone(), customer);
    }

    /// Pre-seeds the gateway with a payment.
    pub fn add_payment(&self, result: PaymentResult) {
        let mut payments = self.payments.write().unwrap();
        payments.insert(result.payment_id.clone(), result);
    }
}

impl PaymentInbound for MockPaymentGateway {
    fn get_payment(&self, payment_id: &str) -> BoxFuture<'_, GatewayResult<PaymentResult>> {
        let id = payment_id.to_string();
        Box::pin(async move {
            let payments = self.payments.read().unwrap();
            payments.get(&id).cloned().ok_or_else(|| {
                GatewayError::NotFound(format!("Payment not found: {}", id))
            })
        })
    }

    fn list_payments(
        &self,
        customer_id: &str,
        limit: usize,
        offset: usize,
    ) -> BoxFuture<'_, GatewayResult<Vec<PaymentResult>>> {
        let customer_id = customer_id.to_string();
        Box::pin(async move {
            let payments = self.payments.read().unwrap();
            let results: Vec<PaymentResult> = payments
                .values()
                .filter(|p| {
                    p.metadata
                        .get("customer_id")
                        .map(|id| id == &customer_id)
                        .unwrap_or(false)
                })
                .skip(offset)
                .take(limit)
                .cloned()
                .collect();
            Ok(results)
        })
    }

    fn get_customer(&self, customer_id: &str) -> BoxFuture<'_, GatewayResult<Customer>> {
        let id = customer_id.to_string();
        Box::pin(async move {
            let customers = self.customers.read().unwrap();
            customers.get(&id).cloned().ok_or_else(|| {
                GatewayError::NotFound(format!("Customer not found: {}", id))
            })
        })
    }

    fn list_payment_methods(
        &self,
        customer_id: &str,
    ) -> BoxFuture<'_, GatewayResult<Vec<PaymentMethod>>> {
        let id = customer_id.to_string();
        Box::pin(async move {
            let methods = self.payment_methods.read().unwrap();
            Ok(methods.get(&id).cloned().unwrap_or_default())
        })
    }

    fn get_refund(&self, refund_id: &str) -> BoxFuture<'_, GatewayResult<RefundResult>> {
        let id = refund_id.to_string();
        Box::pin(async move {
            let refunds = self.refunds.read().unwrap();
            refunds.get(&id).cloned().ok_or_else(|| {
                GatewayError::NotFound(format!("Refund not found: {}", id))
            })
        })
    }

    fn health_check(&self) -> BoxFuture<'_, GatewayResult<HealthCheck>> {
        Box::pin(async move { Ok(HealthCheck::healthy()) })
    }
}

impl PaymentOutbound for MockPaymentGateway {
    fn create_payment(&self, payment: Payment) -> BoxFuture<'_, GatewayResult<PaymentResult>> {
        Box::pin(async move {
            // Check for simulated failures
            if let Some(error_msg) = self.should_fail(&payment) {
                let result = PaymentResult::failed(&payment.id, error_msg);
                let mut payments = self.payments.write().unwrap();
                payments.insert(payment.id.clone(), result.clone());
                return Ok(result);
            }

            // Create successful payment
            let provider_id = format!("mock_pi_{}", uuid::Uuid::new_v4());
            let mut result = PaymentResult::success(&payment.id, &provider_id, payment.amount);

            // Add customer_id to metadata if present
            if let Some(customer_id) = &payment.customer_id {
                result.metadata.insert("customer_id".to_string(), customer_id.clone());
            }

            // Store the payment
            let mut payments = self.payments.write().unwrap();
            payments.insert(payment.id.clone(), result.clone());

            Ok(result)
        })
    }

    fn capture_payment(
        &self,
        payment_id: &str,
        amount: Option<Money>,
    ) -> BoxFuture<'_, GatewayResult<PaymentResult>> {
        let id = payment_id.to_string();
        Box::pin(async move {
            let mut payments = self.payments.write().unwrap();

            let payment = payments.get_mut(&id).ok_or_else(|| {
                GatewayError::NotFound(format!("Payment not found: {}", id))
            })?;

            // Update status to succeeded (captured)
            payment.status = PaymentStatus::Succeeded;
            if let Some(amt) = amount {
                payment.amount = amt;
            }

            Ok(payment.clone())
        })
    }

    fn cancel_payment(&self, payment_id: &str) -> BoxFuture<'_, GatewayResult<PaymentResult>> {
        let id = payment_id.to_string();
        Box::pin(async move {
            let mut payments = self.payments.write().unwrap();

            let payment = payments.get_mut(&id).ok_or_else(|| {
                GatewayError::NotFound(format!("Payment not found: {}", id))
            })?;

            payment.status = PaymentStatus::Canceled;
            Ok(payment.clone())
        })
    }

    fn create_refund(&self, refund: Refund) -> BoxFuture<'_, GatewayResult<RefundResult>> {
        Box::pin(async move {
            // Get the original payment
            let payments = self.payments.read().unwrap();
            let payment = payments.get(&refund.payment_id).ok_or_else(|| {
                GatewayError::NotFound(format!("Payment not found: {}", refund.payment_id))
            })?;

            // Determine refund amount
            let amount = refund.amount.unwrap_or(payment.amount);

            // Create refund result
            let result = RefundResult::success(&refund.id, &refund.payment_id, amount);

            // Store the refund
            let mut refunds = self.refunds.write().unwrap();
            refunds.insert(refund.id.clone(), result.clone());

            // Update payment status
            drop(payments);
            let mut payments = self.payments.write().unwrap();
            if let Some(p) = payments.get_mut(&refund.payment_id) {
                if refund.amount.is_some() && refund.amount.unwrap().amount < p.amount.amount {
                    p.status = PaymentStatus::PartiallyRefunded;
                } else {
                    p.status = PaymentStatus::Refunded;
                }
            }

            Ok(result)
        })
    }

    fn create_customer(&self, customer: Customer) -> BoxFuture<'_, GatewayResult<Customer>> {
        Box::pin(async move {
            let mut customers = self.customers.write().unwrap();
            customers.insert(customer.id.clone(), customer.clone());
            Ok(customer)
        })
    }

    fn update_customer(&self, customer: Customer) -> BoxFuture<'_, GatewayResult<Customer>> {
        let id = customer.id.clone();
        Box::pin(async move {
            let mut customers = self.customers.write().unwrap();

            if !customers.contains_key(&id) {
                return Err(GatewayError::NotFound(format!(
                    "Customer not found: {}",
                    id
                )));
            }

            customers.insert(id, customer.clone());
            Ok(customer)
        })
    }

    fn delete_customer(&self, customer_id: &str) -> BoxFuture<'_, GatewayResult<()>> {
        let id = customer_id.to_string();
        Box::pin(async move {
            let mut customers = self.customers.write().unwrap();
            customers.remove(&id);

            // Also remove payment methods
            let mut methods = self.payment_methods.write().unwrap();
            methods.remove(&id);

            Ok(())
        })
    }
}

impl PaymentGateway for MockPaymentGateway {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::payment::RefundStatus;

    #[tokio::test]
    async fn test_create_payment() {
        let gateway = MockPaymentGateway::new();

        let payment = Payment::new(Money::usd(1000))
            .with_description("Test payment");

        let result = gateway.create_payment(payment.clone()).await.unwrap();

        assert_eq!(result.payment_id, payment.id);
        assert_eq!(result.status, PaymentStatus::Succeeded);
        assert_eq!(result.amount.amount, 1000);
    }

    /// @covers: with_failure_mode
    #[tokio::test]
    async fn test_payment_failure_mode() {
        let gateway = MockPaymentGateway::new()
            .with_failure_mode(MockFailureMode::FailOverAmount(500));

        let small_payment = Payment::new(Money::usd(400));
        let result = gateway.create_payment(small_payment).await.unwrap();
        assert_eq!(result.status, PaymentStatus::Succeeded);

        let large_payment = Payment::new(Money::usd(600));
        let result = gateway.create_payment(large_payment).await.unwrap();
        assert_eq!(result.status, PaymentStatus::Failed);
    }

    #[tokio::test]
    async fn test_refund() {
        let gateway = MockPaymentGateway::new();

        let payment = Payment::new(Money::usd(1000));
        gateway.create_payment(payment.clone()).await.unwrap();

        let refund = Refund::full(&payment.id);
        let result = gateway.create_refund(refund).await.unwrap();

        assert_eq!(result.status, RefundStatus::Succeeded);
        assert_eq!(result.amount.amount, 1000);

        // Check payment status is updated
        let payment_status = gateway.get_payment(&payment.id).await.unwrap();
        assert_eq!(payment_status.status, PaymentStatus::Refunded);
    }

    #[tokio::test]
    async fn test_customer_crud() {
        let gateway = MockPaymentGateway::new();

        let customer = Customer::new("test@example.com").with_name("Test User");

        // Create
        let created = gateway.create_customer(customer.clone()).await.unwrap();
        assert_eq!(created.email, Some("test@example.com".to_string()));

        // Read
        let retrieved = gateway.get_customer(&customer.id).await.unwrap();
        assert_eq!(retrieved.name, Some("Test User".to_string()));

        // Update
        let mut updated = retrieved.clone();
        updated.name = Some("Updated Name".to_string());
        gateway.update_customer(updated.clone()).await.unwrap();

        let retrieved = gateway.get_customer(&customer.id).await.unwrap();
        assert_eq!(retrieved.name, Some("Updated Name".to_string()));

        // Delete
        gateway.delete_customer(&customer.id).await.unwrap();
        assert!(gateway.get_customer(&customer.id).await.is_err());
    }

    /// @covers: with_failure_mode
    #[test]
    fn test_with_failure_mode_sets_mode() {
        let gateway = MockPaymentGateway::new()
            .with_failure_mode(MockFailureMode::FailAllPayments("blocked".into()));
        let mode = gateway.failure_mode.read().unwrap();
        assert!(mode.is_some(), "with_failure_mode should set the failure mode");
        match mode.as_ref().unwrap() {
            MockFailureMode::FailAllPayments(msg) => {
                assert_eq!(msg, "blocked", "failure message should match");
            }
            _ => panic!("expected FailAllPayments variant"),
        }
    }

    /// @covers: with_failure_mode
    #[tokio::test]
    async fn test_with_failure_mode() {
        let gateway = MockPaymentGateway::new()
            .with_failure_mode(MockFailureMode::FailAllPayments("blocked".into()));
        let payment = Payment::new(Money::usd(100));
        let result = gateway.create_payment(payment).await.unwrap();
        assert_eq!(result.status, PaymentStatus::Failed);
        assert!(result.error_message.as_ref().unwrap().contains("blocked"));
    }

    /// @covers: clear_failure_mode
    #[test]
    fn test_clear_failure_mode_removes_mode() {
        let gateway = MockPaymentGateway::new()
            .with_failure_mode(MockFailureMode::FailAllPayments("err".into()));
        // Verify failure mode is set
        assert!(gateway.failure_mode.read().unwrap().is_some());

        gateway.clear_failure_mode();

        // Verify failure mode is cleared
        assert!(
            gateway.failure_mode.read().unwrap().is_none(),
            "clear_failure_mode should remove the failure mode"
        );
    }

    /// @covers: clear_failure_mode
    #[tokio::test]
    async fn test_clear_failure_mode() {
        let gateway = MockPaymentGateway::new()
            .with_failure_mode(MockFailureMode::FailAllPayments("err".into()));
        gateway.clear_failure_mode();
        let payment = Payment::new(Money::usd(100));
        let result = gateway.create_payment(payment).await.unwrap();
        assert_eq!(result.status, PaymentStatus::Succeeded);
    }

    /// @covers: add_customer
    #[test]
    fn test_add_customer_seeds_store() {
        let gateway = MockPaymentGateway::new();
        let customer = Customer::new("seed@test.com").with_name("Seed");
        let id = customer.id.clone();
        gateway.add_customer(customer);

        let customers = gateway.customers.read().unwrap();
        assert!(customers.contains_key(&id), "add_customer should store the customer");
        assert_eq!(
            customers[&id].name,
            Some("Seed".to_string()),
            "stored customer should have correct name"
        );
    }

    /// @covers: add_customer
    #[tokio::test]
    async fn test_add_customer() {
        let gateway = MockPaymentGateway::new();
        let customer = Customer::new("seed@test.com").with_name("Seed");
        let id = customer.id.clone();
        gateway.add_customer(customer);
        let retrieved = gateway.get_customer(&id).await.unwrap();
        assert_eq!(retrieved.name, Some("Seed".to_string()));
    }

    /// @covers: add_payment
    #[test]
    fn test_add_payment_seeds_store() {
        let gateway = MockPaymentGateway::new();
        let result = PaymentResult::success("pay-seed", "prov-1", Money::usd(999));
        gateway.add_payment(result);

        let payments = gateway.payments.read().unwrap();
        assert!(
            payments.contains_key("pay-seed"),
            "add_payment should store the payment"
        );
        assert_eq!(
            payments["pay-seed"].amount.amount, 999,
            "stored payment should have correct amount"
        );
    }

    /// @covers: add_payment
    #[tokio::test]
    async fn test_add_payment() {
        let gateway = MockPaymentGateway::new();
        let result = PaymentResult::success("pay-seed", "prov-1", Money::usd(999));
        gateway.add_payment(result);
        let retrieved = gateway.get_payment("pay-seed").await.unwrap();
        assert_eq!(retrieved.amount.amount, 999);
        assert_eq!(retrieved.status, PaymentStatus::Succeeded);
    }
}
