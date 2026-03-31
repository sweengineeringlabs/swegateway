//! Payment gateway types and configuration.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Payment provider type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaymentProvider {
    /// Mock provider for testing.
    Mock,
    /// Stripe.
    Stripe,
    /// PayPal.
    PayPal,
    /// Square.
    Square,
    /// Braintree.
    Braintree,
}

impl Default for PaymentProvider {
    fn default() -> Self {
        Self::Mock
    }
}

/// Configuration for payment processing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PaymentConfig {
    /// Payment provider.
    pub provider: PaymentProvider,
    /// API key or public key.
    #[serde(skip_serializing)]
    pub api_key: Option<String>,
    /// Secret key.
    #[serde(skip_serializing)]
    pub secret_key: Option<String>,
    /// Webhook secret for signature verification.
    #[serde(skip_serializing)]
    pub webhook_secret: Option<String>,
    /// Sandbox/test mode.
    pub sandbox: bool,
    /// Default currency.
    pub default_currency: Currency,
    /// Additional options.
    #[serde(default)]
    pub options: HashMap<String, String>,
}

impl Default for PaymentConfig {
    fn default() -> Self {
        Self {
            provider: PaymentProvider::Mock,
            api_key: None,
            secret_key: None,
            webhook_secret: None,
            sandbox: true,
            default_currency: Currency::Usd,
            options: HashMap::new(),
        }
    }
}

impl PaymentConfig {
    /// Creates a mock payment configuration.
    pub fn mock() -> Self {
        Self::default()
    }

    /// Creates a Stripe configuration.
    pub fn stripe(secret_key: impl Into<String>) -> Self {
        Self {
            provider: PaymentProvider::Stripe,
            secret_key: Some(secret_key.into()),
            ..Default::default()
        }
    }
}

/// Currency code (ISO 4217).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum Currency {
    Usd,
    Eur,
    Gbp,
    Cad,
    Aud,
    Jpy,
    Cny,
    Inr,
    Brl,
    Mxn,
}

impl Default for Currency {
    fn default() -> Self {
        Self::Usd
    }
}

impl std::fmt::Display for Currency {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usd => write!(f, "USD"),
            Self::Eur => write!(f, "EUR"),
            Self::Gbp => write!(f, "GBP"),
            Self::Cad => write!(f, "CAD"),
            Self::Aud => write!(f, "AUD"),
            Self::Jpy => write!(f, "JPY"),
            Self::Cny => write!(f, "CNY"),
            Self::Inr => write!(f, "INR"),
            Self::Brl => write!(f, "BRL"),
            Self::Mxn => write!(f, "MXN"),
        }
    }
}

/// A monetary amount with currency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    /// Amount in the smallest currency unit (e.g., cents for USD).
    pub amount: i64,
    /// Currency code.
    pub currency: Currency,
}

impl Money {
    /// Creates a new Money value.
    pub fn new(amount: i64, currency: Currency) -> Self {
        Self { amount, currency }
    }

    /// Creates a USD amount.
    pub fn usd(cents: i64) -> Self {
        Self::new(cents, Currency::Usd)
    }

    /// Creates a EUR amount.
    pub fn eur(cents: i64) -> Self {
        Self::new(cents, Currency::Eur)
    }

    /// Creates from a decimal amount (e.g., 10.50 -> 1050 cents).
    pub fn from_decimal(amount: f64, currency: Currency) -> Self {
        Self {
            amount: (amount * 100.0).round() as i64,
            currency,
        }
    }

    /// Returns the decimal representation.
    pub fn to_decimal(&self) -> f64 {
        self.amount as f64 / 100.0
    }

    /// Returns true if the amount is zero.
    pub fn is_zero(&self) -> bool {
        self.amount == 0
    }

    /// Returns true if the amount is positive.
    pub fn is_positive(&self) -> bool {
        self.amount > 0
    }
}

impl std::fmt::Display for Money {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.2} {}", self.to_decimal(), self.currency)
    }
}

/// A payment intent/request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Payment {
    /// Unique payment ID.
    pub id: String,
    /// Payment amount.
    pub amount: Money,
    /// Payment description.
    pub description: Option<String>,
    /// Customer ID.
    pub customer_id: Option<String>,
    /// Payment method ID.
    pub payment_method_id: Option<String>,
    /// Statement descriptor.
    pub statement_descriptor: Option<String>,
    /// Additional metadata.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    /// Idempotency key.
    pub idempotency_key: Option<String>,
}

impl Payment {
    /// Creates a new payment.
    pub fn new(amount: Money) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            amount,
            description: None,
            customer_id: None,
            payment_method_id: None,
            statement_descriptor: None,
            metadata: HashMap::new(),
            idempotency_key: None,
        }
    }

    /// Sets the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Sets the customer ID.
    pub fn with_customer(mut self, customer_id: impl Into<String>) -> Self {
        self.customer_id = Some(customer_id.into());
        self
    }

    /// Sets the payment method.
    pub fn with_payment_method(mut self, payment_method_id: impl Into<String>) -> Self {
        self.payment_method_id = Some(payment_method_id.into());
        self
    }

    /// Adds metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.metadata.insert(key.into(), value.into());
        self
    }

    /// Sets the idempotency key.
    pub fn with_idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = Some(key.into());
        self
    }
}

/// Result of a payment operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentResult {
    /// Payment ID.
    pub payment_id: String,
    /// Provider's payment ID.
    pub provider_id: Option<String>,
    /// Payment status.
    pub status: PaymentStatus,
    /// Amount charged.
    pub amount: Money,
    /// Fee charged by provider.
    pub fee: Option<Money>,
    /// Net amount after fees.
    pub net_amount: Option<Money>,
    /// Error message if failed.
    pub error_message: Option<String>,
    /// Error code if failed.
    pub error_code: Option<String>,
    /// Timestamp.
    pub created_at: DateTime<Utc>,
    /// Additional metadata.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl PaymentResult {
    /// Creates a successful payment result.
    pub fn success(payment_id: impl Into<String>, provider_id: impl Into<String>, amount: Money) -> Self {
        Self {
            payment_id: payment_id.into(),
            provider_id: Some(provider_id.into()),
            status: PaymentStatus::Succeeded,
            amount,
            fee: None,
            net_amount: None,
            error_message: None,
            error_code: None,
            created_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Creates a failed payment result.
    pub fn failed(payment_id: impl Into<String>, error_message: impl Into<String>) -> Self {
        Self {
            payment_id: payment_id.into(),
            provider_id: None,
            status: PaymentStatus::Failed,
            amount: Money::usd(0),
            fee: None,
            net_amount: None,
            error_message: Some(error_message.into()),
            error_code: None,
            created_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }

    /// Creates a pending payment result.
    pub fn pending(payment_id: impl Into<String>, amount: Money) -> Self {
        Self {
            payment_id: payment_id.into(),
            provider_id: None,
            status: PaymentStatus::Pending,
            amount,
            fee: None,
            net_amount: None,
            error_message: None,
            error_code: None,
            created_at: Utc::now(),
            metadata: HashMap::new(),
        }
    }
}

/// Payment status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PaymentStatus {
    /// Payment is pending.
    Pending,
    /// Payment requires action (e.g., 3DS).
    RequiresAction,
    /// Payment is processing.
    Processing,
    /// Payment succeeded.
    Succeeded,
    /// Payment failed.
    Failed,
    /// Payment was canceled.
    Canceled,
    /// Payment was refunded.
    Refunded,
    /// Payment was partially refunded.
    PartiallyRefunded,
}

/// A refund request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Refund {
    /// Unique refund ID.
    pub id: String,
    /// Original payment ID.
    pub payment_id: String,
    /// Refund amount (optional, full refund if not specified).
    pub amount: Option<Money>,
    /// Reason for refund.
    pub reason: Option<RefundReason>,
    /// Additional description.
    pub description: Option<String>,
    /// Additional metadata.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

impl Refund {
    /// Creates a full refund.
    pub fn full(payment_id: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            payment_id: payment_id.into(),
            amount: None,
            reason: None,
            description: None,
            metadata: HashMap::new(),
        }
    }

    /// Creates a partial refund.
    pub fn partial(payment_id: impl Into<String>, amount: Money) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            payment_id: payment_id.into(),
            amount: Some(amount),
            reason: None,
            description: None,
            metadata: HashMap::new(),
        }
    }

    /// Sets the refund reason.
    pub fn with_reason(mut self, reason: RefundReason) -> Self {
        self.reason = Some(reason);
        self
    }

    /// Sets the description.
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// Refund reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RefundReason {
    /// Duplicate charge.
    Duplicate,
    /// Fraudulent charge.
    Fraudulent,
    /// Customer requested.
    CustomerRequest,
    /// Other reason.
    Other,
}

/// Result of a refund operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefundResult {
    /// Refund ID.
    pub refund_id: String,
    /// Provider's refund ID.
    pub provider_id: Option<String>,
    /// Original payment ID.
    pub payment_id: String,
    /// Refund status.
    pub status: RefundStatus,
    /// Amount refunded.
    pub amount: Money,
    /// Error message if failed.
    pub error_message: Option<String>,
    /// Timestamp.
    pub created_at: DateTime<Utc>,
}

impl RefundResult {
    /// Creates a successful refund result.
    pub fn success(refund_id: impl Into<String>, payment_id: impl Into<String>, amount: Money) -> Self {
        Self {
            refund_id: refund_id.into(),
            provider_id: None,
            payment_id: payment_id.into(),
            status: RefundStatus::Succeeded,
            amount,
            error_message: None,
            created_at: Utc::now(),
        }
    }

    /// Creates a failed refund result.
    pub fn failed(refund_id: impl Into<String>, payment_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            refund_id: refund_id.into(),
            provider_id: None,
            payment_id: payment_id.into(),
            status: RefundStatus::Failed,
            amount: Money::usd(0),
            error_message: Some(error.into()),
            created_at: Utc::now(),
        }
    }
}

/// Refund status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RefundStatus {
    /// Refund is pending.
    Pending,
    /// Refund succeeded.
    Succeeded,
    /// Refund failed.
    Failed,
    /// Refund was canceled.
    Canceled,
}

/// Customer information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Customer {
    /// Customer ID.
    pub id: String,
    /// Email address.
    pub email: Option<String>,
    /// Customer name.
    pub name: Option<String>,
    /// Phone number.
    pub phone: Option<String>,
    /// Additional metadata.
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
}

impl Customer {
    /// Creates a new customer.
    pub fn new(email: impl Into<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            email: Some(email.into()),
            name: None,
            phone: None,
            metadata: HashMap::new(),
            created_at: Utc::now(),
        }
    }

    /// Sets the name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Sets the phone.
    pub fn with_phone(mut self, phone: impl Into<String>) -> Self {
        self.phone = Some(phone.into());
        self
    }
}

/// Payment method information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentMethod {
    /// Payment method ID.
    pub id: String,
    /// Payment method type.
    pub method_type: PaymentMethodType,
    /// Customer ID this method belongs to.
    pub customer_id: Option<String>,
    /// Card details (for card methods).
    pub card: Option<CardDetails>,
    /// Whether this is the default method.
    pub is_default: bool,
    /// Created timestamp.
    pub created_at: DateTime<Utc>,
}

/// Payment method type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentMethodType {
    /// Credit/debit card.
    Card,
    /// Bank account.
    BankAccount,
    /// Digital wallet.
    Wallet,
}

/// Card details (sanitized).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CardDetails {
    /// Card brand (visa, mastercard, etc.).
    pub brand: String,
    /// Last 4 digits.
    pub last_four: String,
    /// Expiration month.
    pub exp_month: u8,
    /// Expiration year.
    pub exp_year: u16,
    /// Country code.
    pub country: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Money tests ──

    /// @covers: usd
    #[test]
    fn test_money_usd() {
        let m = Money::usd(1050);
        assert_eq!(m.amount, 1050);
        assert_eq!(m.currency, Currency::Usd);
        assert!((m.to_decimal() - 10.50).abs() < f64::EPSILON);
    }

    /// @covers: from_decimal
    #[test]
    fn test_money_from_decimal() {
        let m = Money::from_decimal(19.99, Currency::Eur);
        assert_eq!(m.amount, 1999);
        assert_eq!(m.currency, Currency::Eur);
    }

    /// @covers: is_zero
    #[test]
    fn test_money_is_zero_and_positive() {
        assert!(Money::usd(0).is_zero());
        assert!(!Money::usd(0).is_positive());
        assert!(Money::usd(100).is_positive());
    }

    #[test]
    fn test_money_display() {
        let m = Money::usd(999);
        assert_eq!(m.to_string(), "9.99 USD");
    }

    #[test]
    fn test_payment_new() {
        let p = Payment::new(Money::usd(2500))
            .with_description("Test charge")
            .with_customer("cust-1");
        assert_eq!(p.amount, Money::usd(2500));
        assert_eq!(p.description, Some("Test charge".to_string()));
        assert_eq!(p.customer_id, Some("cust-1".to_string()));
        assert!(!p.id.is_empty());
    }

    #[test]
    fn test_currency_display() {
        assert_eq!(Currency::Usd.to_string(), "USD");
        assert_eq!(Currency::Eur.to_string(), "EUR");
        assert_eq!(Currency::Jpy.to_string(), "JPY");
    }

    // ── PaymentConfig tests ──

    /// @covers: mock
    #[test]
    fn test_mock() {
        let cfg = PaymentConfig::mock();
        assert_eq!(cfg.provider, PaymentProvider::Mock);
        assert!(cfg.sandbox);
        assert!(cfg.api_key.is_none());
        assert!(cfg.secret_key.is_none());
    }

    /// @covers: stripe
    #[test]
    fn test_stripe() {
        let cfg = PaymentConfig::stripe("sk_test_xxx");
        assert_eq!(cfg.provider, PaymentProvider::Stripe);
        assert_eq!(cfg.secret_key, Some("sk_test_xxx".to_string()));
    }

    // ── Money (additional) tests ──

    /// @covers: eur
    #[test]
    fn test_eur() {
        let m = Money::eur(500);
        assert_eq!(m.currency, Currency::Eur);
        assert_eq!(m.amount, 500);
    }

    /// @covers: to_decimal
    #[test]
    fn test_to_decimal() {
        let m = Money::usd(1050);
        let d = m.to_decimal();
        assert!((d - 10.50).abs() < f64::EPSILON, "expected 10.50, got {d}");
    }

    /// @covers: is_positive
    #[test]
    fn test_is_positive() {
        assert!(Money::usd(100).is_positive());
        assert!(!Money::usd(0).is_positive());
        assert!(!Money::usd(-1).is_positive());
    }

    // ── Payment builder tests ──

    /// @covers: with_description
    #[test]
    fn test_with_description() {
        let p = Payment::new(Money::usd(100)).with_description("test");
        assert_eq!(p.description, Some("test".to_string()));
    }

    /// @covers: with_customer
    #[test]
    fn test_with_customer() {
        let p = Payment::new(Money::usd(100)).with_customer("cust-42");
        assert_eq!(p.customer_id, Some("cust-42".to_string()));
    }

    /// @covers: with_payment_method
    #[test]
    fn test_with_payment_method() {
        let p = Payment::new(Money::usd(100)).with_payment_method("pm-1");
        assert_eq!(p.payment_method_id, Some("pm-1".to_string()));
    }

    /// @covers: with_metadata
    #[test]
    fn test_with_metadata() {
        let p = Payment::new(Money::usd(100)).with_metadata("order_id", "ord-1");
        assert_eq!(p.metadata.get("order_id"), Some(&"ord-1".to_string()));
    }

    /// @covers: with_idempotency_key
    #[test]
    fn test_with_idempotency_key() {
        let p = Payment::new(Money::usd(100)).with_idempotency_key("idem-1");
        assert_eq!(p.idempotency_key, Some("idem-1".to_string()));
    }

    // ── PaymentResult tests ──

    /// @covers: success
    #[test]
    fn test_payment_result_success() {
        let r = PaymentResult::success("p1", "prov1", Money::usd(100));
        assert_eq!(r.status, PaymentStatus::Succeeded);
        assert_eq!(r.provider_id, Some("prov1".to_string()));
        assert_eq!(r.payment_id, "p1");
        assert_eq!(r.amount, Money::usd(100));
        assert!(r.error_message.is_none());
    }

    /// @covers: failed
    #[test]
    fn test_payment_result_failed() {
        let r = PaymentResult::failed("p1", "card_declined");
        assert_eq!(r.status, PaymentStatus::Failed);
        assert_eq!(r.error_message, Some("card_declined".to_string()));
        assert!(r.provider_id.is_none());
    }

    /// @covers: pending
    #[test]
    fn test_pending() {
        let r = PaymentResult::pending("p1", Money::usd(100));
        assert_eq!(r.status, PaymentStatus::Pending);
        assert_eq!(r.amount, Money::usd(100));
        assert!(r.provider_id.is_none());
        assert!(r.error_message.is_none());
    }

    // ── Refund tests ──

    /// @covers: full
    #[test]
    fn test_full() {
        let r = Refund::full("pay-1");
        assert_eq!(r.payment_id, "pay-1");
        assert!(r.amount.is_none());
        assert!(!r.id.is_empty());
    }

    /// @covers: partial
    #[test]
    fn test_partial() {
        let r = Refund::partial("pay-1", Money::usd(500));
        assert_eq!(r.payment_id, "pay-1");
        assert_eq!(r.amount, Some(Money::usd(500)));
    }

    /// @covers: with_reason
    #[test]
    fn test_with_reason() {
        let r = Refund::full("p1").with_reason(RefundReason::Duplicate);
        assert_eq!(r.reason, Some(RefundReason::Duplicate));
    }

    /// @covers: with_description
    #[test]
    fn test_refund_with_description() {
        let r = Refund::full("p1").with_description("desc");
        assert_eq!(r.description, Some("desc".to_string()));
    }

    // ── RefundResult tests ──

    /// @covers: success
    #[test]
    fn test_refund_result_success() {
        let r = RefundResult::success("r1", "p1", Money::usd(100));
        assert_eq!(r.status, RefundStatus::Succeeded);
        assert_eq!(r.refund_id, "r1");
        assert_eq!(r.payment_id, "p1");
        assert_eq!(r.amount, Money::usd(100));
        assert!(r.error_message.is_none());
    }

    /// @covers: failed
    #[test]
    fn test_refund_result_failed() {
        let r = RefundResult::failed("r1", "p1", "already_refunded");
        assert_eq!(r.status, RefundStatus::Failed);
        assert_eq!(r.error_message, Some("already_refunded".to_string()));
        assert_eq!(r.refund_id, "r1");
        assert_eq!(r.payment_id, "p1");
    }

    // ── Customer tests ──

    /// @covers: with_name
    #[test]
    fn test_with_name() {
        let c = Customer::new("e@x.com").with_name("Bob");
        assert_eq!(c.name, Some("Bob".to_string()));
        assert_eq!(c.email, Some("e@x.com".to_string()));
    }

    /// @covers: with_phone
    #[test]
    fn test_with_phone() {
        let c = Customer::new("e@x.com").with_phone("+1234");
        assert_eq!(c.phone, Some("+1234".to_string()));
    }
}
