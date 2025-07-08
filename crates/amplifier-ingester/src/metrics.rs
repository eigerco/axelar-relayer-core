use opentelemetry::metrics::Counter;
use opentelemetry::{KeyValue, global};

/// Metrics specifically for tracking Amplifier API event processing.
///
/// This struct provides instrumentation for Amplifier API components, tracking
/// different event types that are processed by the amplifier ingester.
pub(crate) struct AmplifierIngesterMetrics {
    // Amplifier API specific event counters - received
    gas_credit_received: Counter<u64>,
    gas_refunded_received: Counter<u64>,
    call_received: Counter<u64>,
    message_approved_received: Counter<u64>,
    message_executed_received: Counter<u64>,
    cannot_execute_message_received: Counter<u64>,
    signers_rotated_received: Counter<u64>,

    // Amplifier API specific event counters - processed
    gas_credit_processed: Counter<u64>,
    gas_refunded_processed: Counter<u64>,
    call_processed: Counter<u64>,
    message_approved_processed: Counter<u64>,
    message_executed_processed: Counter<u64>,
    cannot_execute_message_processed: Counter<u64>,
    signers_rotated_processed: Counter<u64>,

    // Error counter
    error_raised: Counter<u64>,

    attributes: Vec<KeyValue>,
}

impl AmplifierIngesterMetrics {
    /// Creates a new `AmplifierIngesterMetrics` instance with the specified meter name and
    /// attributes.
    ///
    /// # Parameters
    ///
    /// * `name` - The name to use for the OpenTelemetry meter, typically identifying the component
    ///   (e.g., `"amplifier_ingester"`).
    /// * `attributes` - Common key-value pairs to attach to all metrics
    ///
    /// # Returns
    ///
    /// A new `AmplifierIngesterMetrics` instance with all counters initialized.
    #[must_use]
    pub(crate) fn new(name: &'static str, attributes: Vec<KeyValue>) -> Self {
        let meter = global::meter(name);

        // Amplifier API specific event counters - received
        let gas_credit_received = meter
            .u64_counter("amplifier.received.gas_credit.count")
            .with_description("Number of received GasCredit events")
            .build();

        let gas_refunded_received = meter
            .u64_counter("amplifier.received.gas_refunded.count")
            .with_description("Number of received GasRefunded events")
            .build();

        let call_received = meter
            .u64_counter("amplifier.received.call.count")
            .with_description("Number of received Call events")
            .build();

        let message_approved_received = meter
            .u64_counter("amplifier.received.message_approved.count")
            .with_description("Number of received MessageApproved events")
            .build();

        let message_executed_received = meter
            .u64_counter("amplifier.received.message_executed.count")
            .with_description("Number of received MessageExecuted events")
            .build();

        let cannot_execute_message_received = meter
            .u64_counter("amplifier.received.cannot_execute_message.count")
            .with_description("Number of received CannotExecuteMessage events")
            .build();

        let signers_rotated_received = meter
            .u64_counter("amplifier.received.signers_rotated.count")
            .with_description("Number of received SignersRotated events")
            .build();

        // Amplifier API specific event counters - processed
        let gas_credit_processed = meter
            .u64_counter("amplifier.processed.gas_credit.count")
            .with_description("Number of processed GasCredit events")
            .build();

        let gas_refunded_processed = meter
            .u64_counter("amplifier.processed.gas_refunded.count")
            .with_description("Number of processed GasRefunded events")
            .build();

        let call_processed = meter
            .u64_counter("amplifier.processed.call.count")
            .with_description("Number of processed Call events")
            .build();

        let message_approved_processed = meter
            .u64_counter("amplifier.processed.message_approved.count")
            .with_description("Number of processed MessageApproved events")
            .build();

        let message_executed_processed = meter
            .u64_counter("amplifier.processed.message_executed.count")
            .with_description("Number of processed MessageExecuted events")
            .build();

        let cannot_execute_message_processed = meter
            .u64_counter("amplifier.processed.cannot_execute_message.count")
            .with_description("Number of processed CannotExecuteMessage events")
            .build();

        let signers_rotated_processed = meter
            .u64_counter("amplifier.processed.signers_rotated.count")
            .with_description("Number of processed SignersRotated events")
            .build();

        let error_raised = meter
            .u64_counter("errors.count")
            .with_description("Total number of errors encountered during operation")
            .build();

        Self {
            // Received events
            gas_credit_received,
            gas_refunded_received,
            call_received,
            message_approved_received,
            message_executed_received,
            cannot_execute_message_received,
            signers_rotated_received,

            // Processed events
            gas_credit_processed,
            gas_refunded_processed,
            call_processed,
            message_approved_processed,
            message_executed_processed,
            cannot_execute_message_processed,
            signers_rotated_processed,

            error_raised,
            attributes,
        }
    }

    // -- Amplifier API event received methods

    /// Records the receipt of a `GasCredit` event.
    pub(crate) fn record_gas_credit_received(&self) {
        self.gas_credit_received.add(1, &self.attributes);
    }

    /// Records the receipt of a `GasRefunded` event.
    pub(crate) fn record_gas_refunded_received(&self) {
        self.gas_refunded_received.add(1, &self.attributes);
    }

    /// Records the receipt of a Call event.
    pub(crate) fn record_call_received(&self) {
        self.call_received.add(1, &self.attributes);
    }

    /// Records the receipt of a `MessageApproved` event.
    pub(crate) fn record_message_approved_received(&self) {
        self.message_approved_received.add(1, &self.attributes);
    }

    /// Records the receipt of a `MessageExecuted` event.
    pub(crate) fn record_message_executed_received(&self) {
        self.message_executed_received.add(1, &self.attributes);
    }

    /// Records the receipt of a `CannotExecuteMessage` event.
    pub(crate) fn record_cannot_execute_message_received(&self) {
        self.cannot_execute_message_received
            .add(1, &self.attributes);
    }

    /// Records the receipt of a `SignersRotated` event.
    pub(crate) fn record_signers_rotated_received(&self) {
        self.signers_rotated_received.add(1, &self.attributes);
    }

    // -- Amplifier API event processed methods

    /// Records the successful processing of a `GasCredit` event.
    pub(crate) fn record_gas_credit_processed(&self) {
        self.gas_credit_processed.add(1, &self.attributes);
    }

    /// Records the successful processing of a `GasRefunded` event.
    pub(crate) fn record_gas_refunded_processed(&self) {
        self.gas_refunded_processed.add(1, &self.attributes);
    }

    /// Records the successful processing of a Call event.
    pub(crate) fn record_call_processed(&self) {
        self.call_processed.add(1, &self.attributes);
    }

    /// Records the successful processing of a `MessageApproved` event.
    pub(crate) fn record_message_approved_processed(&self) {
        self.message_approved_processed.add(1, &self.attributes);
    }

    /// Records the successful processing of a `MessageExecuted` event.
    pub(crate) fn record_message_executed_processed(&self) {
        self.message_executed_processed.add(1, &self.attributes);
    }

    /// Records the successful processing of a `CannotExecuteMessage` event.
    pub(crate) fn record_cannot_execute_message_processed(&self) {
        self.cannot_execute_message_processed
            .add(1, &self.attributes);
    }

    /// Records the successful processing of a `SignersRotated` event.
    pub(crate) fn record_signers_rotated_processed(&self) {
        self.signers_rotated_processed.add(1, &self.attributes);
    }

    // -- Error and skip tracking

    /// Records an error encountered during task processing.
    pub(crate) fn record_error(&self) {
        self.error_raised.add(1, &self.attributes);
    }
}
