//! Gateway traits defining the core abstractions.
//!
//! Each gateway has three traits:
//! - `*Inbound`: Read operations (queries, fetches)
//! - `*Outbound`: Write operations (creates, updates, deletes)
//! - `*Gateway`: Combined trait extending both Inbound and Outbound

use std::pin::Pin;

use futures::future::BoxFuture;
use futures::stream::Stream;

use crate::api::{
    database::{QueryParams, Record, WriteResult},
    file::{FileInfo, ListOptions, ListResult, PresignedUrl, UploadOptions},
    grpc,
    http::{HttpRequest, HttpResponse},
    notification::{Notification, NotificationReceipt},
    payment::{Customer, Money, Payment, PaymentMethod, PaymentResult, Refund, RefundResult},
    types::{GatewayResult, HealthCheck},
};

// =============================================================================
// Database Gateway
// =============================================================================

/// Inbound operations for database access (read operations).
pub trait DatabaseInbound: Send + Sync {
    /// Queries records from a table/collection.
    fn query(
        &self,
        table: &str,
        params: QueryParams,
    ) -> BoxFuture<'_, GatewayResult<Vec<Record>>>;

    /// Gets a single record by ID.
    fn get_by_id(
        &self,
        table: &str,
        id: &str,
    ) -> BoxFuture<'_, GatewayResult<Option<Record>>>;

    /// Checks if a record exists.
    fn exists(&self, table: &str, id: &str) -> BoxFuture<'_, GatewayResult<bool>>;

    /// Counts records matching the query.
    fn count(&self, table: &str, params: QueryParams) -> BoxFuture<'_, GatewayResult<u64>>;

    /// Performs a health check on the database connection.
    fn health_check(&self) -> BoxFuture<'_, GatewayResult<HealthCheck>>;

    /// Streams records from a table/collection one by one.
    ///
    /// The default implementation calls [`query`](Self::query) and converts
    /// the resulting `Vec` into a stream. Implementations may override this
    /// to provide true incremental streaming for large result sets.
    fn query_stream(
        &self,
        table: &str,
        params: QueryParams,
    ) -> BoxFuture<'_, GatewayResult<Pin<Box<dyn Stream<Item = GatewayResult<Record>> + Send + '_>>>>
    {
        let table = table.to_string();
        Box::pin(async move {
            let records = self.query(&table, params).await?;
            let stream: Pin<Box<dyn Stream<Item = GatewayResult<Record>> + Send + '_>> =
                Box::pin(futures::stream::iter(records.into_iter().map(Ok)));
            Ok(stream)
        })
    }
}

/// Outbound operations for database access (write operations).
pub trait DatabaseOutbound: Send + Sync {
    /// Inserts a new record.
    fn insert(
        &self,
        table: &str,
        record: Record,
    ) -> BoxFuture<'_, GatewayResult<WriteResult>>;

    /// Updates an existing record by ID.
    fn update(
        &self,
        table: &str,
        id: &str,
        record: Record,
    ) -> BoxFuture<'_, GatewayResult<WriteResult>>;

    /// Deletes a record by ID.
    fn delete(&self, table: &str, id: &str) -> BoxFuture<'_, GatewayResult<WriteResult>>;

    /// Inserts multiple records in a batch.
    fn batch_insert(
        &self,
        table: &str,
        records: Vec<Record>,
    ) -> BoxFuture<'_, GatewayResult<WriteResult>>;

    /// Updates multiple records matching a query.
    fn update_where(
        &self,
        table: &str,
        params: QueryParams,
        updates: Record,
    ) -> BoxFuture<'_, GatewayResult<WriteResult>>;

    /// Deletes multiple records matching a query.
    fn delete_where(
        &self,
        table: &str,
        params: QueryParams,
    ) -> BoxFuture<'_, GatewayResult<WriteResult>>;
}

/// Combined database gateway trait.
pub trait DatabaseGateway: DatabaseInbound + DatabaseOutbound {}

// =============================================================================
// File Gateway
// =============================================================================

/// Inbound operations for file storage (read operations).
pub trait FileInbound: Send + Sync {
    /// Reads file contents.
    fn read(&self, path: &str) -> BoxFuture<'_, GatewayResult<Vec<u8>>>;

    /// Gets file metadata.
    fn metadata(&self, path: &str) -> BoxFuture<'_, GatewayResult<FileInfo>>;

    /// Lists files in a directory/prefix.
    fn list(&self, options: ListOptions) -> BoxFuture<'_, GatewayResult<ListResult>>;

    /// Checks if a file exists.
    fn exists(&self, path: &str) -> BoxFuture<'_, GatewayResult<bool>>;

    /// Generates a presigned URL for reading.
    fn presigned_read_url(
        &self,
        path: &str,
        expires_in_secs: u64,
    ) -> BoxFuture<'_, GatewayResult<PresignedUrl>>;

    /// Performs a health check.
    fn health_check(&self) -> BoxFuture<'_, GatewayResult<HealthCheck>>;

    /// Streams file listing results one by one.
    ///
    /// The default implementation calls [`list`](Self::list) and converts
    /// the resulting `Vec<FileInfo>` into a stream. Implementations may
    /// override this to provide true incremental streaming for large directories.
    fn list_stream(
        &self,
        options: ListOptions,
    ) -> BoxFuture<'_, GatewayResult<Pin<Box<dyn Stream<Item = GatewayResult<FileInfo>> + Send + '_>>>>
    {
        Box::pin(async move {
            let result = self.list(options).await?;
            let stream: Pin<Box<dyn Stream<Item = GatewayResult<FileInfo>> + Send + '_>> =
                Box::pin(futures::stream::iter(result.files.into_iter().map(Ok)));
            Ok(stream)
        })
    }
}

/// Outbound operations for file storage (write operations).
pub trait FileOutbound: Send + Sync {
    /// Writes file contents.
    fn write(
        &self,
        path: &str,
        contents: Vec<u8>,
        options: UploadOptions,
    ) -> BoxFuture<'_, GatewayResult<FileInfo>>;

    /// Deletes a file.
    fn delete(&self, path: &str) -> BoxFuture<'_, GatewayResult<()>>;

    /// Copies a file.
    fn copy(&self, source: &str, destination: &str) -> BoxFuture<'_, GatewayResult<FileInfo>>;

    /// Moves/renames a file.
    fn rename(&self, source: &str, destination: &str) -> BoxFuture<'_, GatewayResult<FileInfo>>;

    /// Creates a directory.
    fn create_directory(&self, path: &str) -> BoxFuture<'_, GatewayResult<()>>;

    /// Deletes a directory (and optionally its contents).
    fn delete_directory(&self, path: &str, recursive: bool) -> BoxFuture<'_, GatewayResult<()>>;

    /// Generates a presigned URL for uploading.
    fn presigned_upload_url(
        &self,
        path: &str,
        expires_in_secs: u64,
    ) -> BoxFuture<'_, GatewayResult<PresignedUrl>>;
}

/// Combined file gateway trait.
pub trait FileGateway: FileInbound + FileOutbound {}

// =============================================================================
// HTTP Gateway
// =============================================================================

/// Inbound operations for HTTP (receiving/handling requests).
pub trait HttpInbound: Send + Sync {
    /// Handles an incoming HTTP request.
    fn handle(&self, request: HttpRequest) -> BoxFuture<'_, GatewayResult<HttpResponse>>;

    /// Performs a health check.
    fn health_check(&self) -> BoxFuture<'_, GatewayResult<HealthCheck>>;
}

/// Outbound operations for HTTP (making requests).
pub trait HttpOutbound: Send + Sync {
    /// Sends an HTTP request.
    fn send(&self, request: HttpRequest) -> BoxFuture<'_, GatewayResult<HttpResponse>>;

    /// Sends a GET request.
    fn get(&self, url: &str) -> BoxFuture<'_, GatewayResult<HttpResponse>>;

    /// Sends a POST request with JSON body.
    fn post_json(
        &self,
        url: &str,
        body: serde_json::Value,
    ) -> BoxFuture<'_, GatewayResult<HttpResponse>>;

    /// Sends a PUT request with JSON body.
    fn put_json(
        &self,
        url: &str,
        body: serde_json::Value,
    ) -> BoxFuture<'_, GatewayResult<HttpResponse>>;

    /// Sends a DELETE request.
    fn delete(&self, url: &str) -> BoxFuture<'_, GatewayResult<HttpResponse>>;
}

/// Combined HTTP gateway trait.
pub trait HttpGateway: HttpInbound + HttpOutbound {}

// =============================================================================
// Notification Gateway
// =============================================================================

/// Inbound operations for notifications (receiving/querying).
pub trait NotificationInbound: Send + Sync {
    /// Gets the status of a sent notification.
    fn get_status(&self, notification_id: &str) -> BoxFuture<'_, GatewayResult<NotificationReceipt>>;

    /// Lists sent notifications (with optional filters).
    fn list_sent(
        &self,
        limit: usize,
        offset: usize,
    ) -> BoxFuture<'_, GatewayResult<Vec<NotificationReceipt>>>;

    /// Performs a health check.
    fn health_check(&self) -> BoxFuture<'_, GatewayResult<HealthCheck>>;
}

/// Outbound operations for notifications (sending).
pub trait NotificationOutbound: Send + Sync {
    /// Sends a notification.
    fn send(&self, notification: Notification) -> BoxFuture<'_, GatewayResult<NotificationReceipt>>;

    /// Sends multiple notifications.
    fn send_batch(
        &self,
        notifications: Vec<Notification>,
    ) -> BoxFuture<'_, GatewayResult<Vec<NotificationReceipt>>>;

    /// Cancels a scheduled notification.
    fn cancel(&self, notification_id: &str) -> BoxFuture<'_, GatewayResult<bool>>;
}

/// Combined notification gateway trait.
pub trait NotificationGateway: NotificationInbound + NotificationOutbound {}

// =============================================================================
// Payment Gateway
// =============================================================================

/// Inbound operations for payments (queries, status checks).
pub trait PaymentInbound: Send + Sync {
    /// Gets payment details.
    fn get_payment(&self, payment_id: &str) -> BoxFuture<'_, GatewayResult<PaymentResult>>;

    /// Lists payments for a customer.
    fn list_payments(
        &self,
        customer_id: &str,
        limit: usize,
        offset: usize,
    ) -> BoxFuture<'_, GatewayResult<Vec<PaymentResult>>>;

    /// Gets customer details.
    fn get_customer(&self, customer_id: &str) -> BoxFuture<'_, GatewayResult<Customer>>;

    /// Lists payment methods for a customer.
    fn list_payment_methods(
        &self,
        customer_id: &str,
    ) -> BoxFuture<'_, GatewayResult<Vec<PaymentMethod>>>;

    /// Gets refund details.
    fn get_refund(&self, refund_id: &str) -> BoxFuture<'_, GatewayResult<RefundResult>>;

    /// Performs a health check.
    fn health_check(&self) -> BoxFuture<'_, GatewayResult<HealthCheck>>;
}

/// Outbound operations for payments (charges, refunds).
pub trait PaymentOutbound: Send + Sync {
    /// Creates a payment (charge).
    fn create_payment(&self, payment: Payment) -> BoxFuture<'_, GatewayResult<PaymentResult>>;

    /// Captures a previously authorized payment.
    fn capture_payment(
        &self,
        payment_id: &str,
        amount: Option<Money>,
    ) -> BoxFuture<'_, GatewayResult<PaymentResult>>;

    /// Cancels a payment.
    fn cancel_payment(&self, payment_id: &str) -> BoxFuture<'_, GatewayResult<PaymentResult>>;

    /// Creates a refund.
    fn create_refund(&self, refund: Refund) -> BoxFuture<'_, GatewayResult<RefundResult>>;

    /// Creates a customer.
    fn create_customer(&self, customer: Customer) -> BoxFuture<'_, GatewayResult<Customer>>;

    /// Updates a customer.
    fn update_customer(&self, customer: Customer) -> BoxFuture<'_, GatewayResult<Customer>>;

    /// Deletes a customer.
    fn delete_customer(&self, customer_id: &str) -> BoxFuture<'_, GatewayResult<()>>;
}

/// Combined payment gateway trait.
pub trait PaymentGateway: PaymentInbound + PaymentOutbound {}

// =============================================================================
// gRPC Gateway
// =============================================================================

/// Inbound operations for gRPC (handling incoming RPCs).
pub trait GrpcInbound: Send + Sync {
    /// Handle a unary gRPC request.
    fn handle_unary(
        &self,
        request: grpc::GrpcRequest,
    ) -> BoxFuture<'_, GatewayResult<grpc::GrpcResponse>>;

    /// Performs a health check on the gRPC service.
    fn health_check(&self) -> BoxFuture<'_, GatewayResult<HealthCheck>>;
}

/// Outbound operations for gRPC (making outgoing RPC calls).
pub trait GrpcOutbound: Send + Sync {
    /// Send a unary gRPC request to a remote service.
    fn call_unary(
        &self,
        endpoint: &str,
        request: grpc::GrpcRequest,
    ) -> BoxFuture<'_, GatewayResult<grpc::GrpcResponse>>;

    /// Performs a health check.
    fn health_check(&self) -> BoxFuture<'_, GatewayResult<HealthCheck>>;
}

/// Combined gRPC gateway trait.
pub trait GrpcGateway: GrpcInbound + GrpcOutbound {}

#[cfg(test)]
mod tests {
    // Trait-only module; tested via integration tests.
}
