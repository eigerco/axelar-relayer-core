use opentelemetry::metrics::Counter;
use opentelemetry::{KeyValue, global};

/// Metrics specifically for tracking Amplifier API subscriber operations.
///
/// This struct provides instrumentation for the Amplifier subscriber component,
/// tracking task fetching, publishing, and error operations.
pub(crate) struct AmplifierSubscriberMetrics {
    // Task fetch and publish counters
    tasks_fetched: Counter<u64>,
    tasks_published: Counter<u64>,
    empty_responses: Counter<u64>,
    fetch_requests: Counter<u64>,
    publish_batches: Counter<u64>,

    // Error counters
    fetch_errors: Counter<u64>,
    publish_errors: Counter<u64>,
    health_check_errors: Counter<u64>,

    // General error counter for compatibility
    error_raised: Counter<u64>,

    attributes: Vec<KeyValue>,
}

impl AmplifierSubscriberMetrics {
    /// Creates a new `AmplifierSubscriberMetrics` instance with the specified meter name and
    /// attributes.
    ///
    /// # Parameters
    ///
    /// * `name` - The name to use for the OpenTelemetry meter, typically identifying the component
    ///   (e.g., `"amplifier_subscriber"`).
    /// * `attributes` - Common key-value pairs to attach to all metrics
    ///
    /// # Returns
    ///
    /// A new `AmplifierSubscriberMetrics` instance with all counters initialized.
    #[must_use]
    pub(crate) fn new(name: &'static str, attributes: Vec<KeyValue>) -> Self {
        let meter = global::meter(name);

        // Task operation counters
        let tasks_fetched = meter
            .u64_counter("subscriber.tasks_fetched.count")
            .with_description("Total number of tasks fetched from Amplifier API")
            .build();

        let tasks_published = meter
            .u64_counter("subscriber.tasks_published.count")
            .with_description("Total number of tasks published to queue")
            .build();

        let empty_responses = meter
            .u64_counter("subscriber.empty_responses.count")
            .with_description("Number of empty responses from Amplifier API")
            .build();

        let fetch_requests = meter
            .u64_counter("subscriber.fetch_requests.count")
            .with_description("Total number of fetch requests made to Amplifier API")
            .build();

        let publish_batches = meter
            .u64_counter("subscriber.publish_batches.count")
            .with_description("Total number of batches published to queue")
            .build();

        // Error counters
        let fetch_errors = meter
            .u64_counter("subscriber.fetch_errors.count")
            .with_description("Number of errors during task fetching")
            .build();

        let publish_errors = meter
            .u64_counter("subscriber.publish_errors.count")
            .with_description("Number of errors during task publishing")
            .build();

        let health_check_errors = meter
            .u64_counter("subscriber.health_check_errors.count")
            .with_description("Number of health check errors")
            .build();

        let error_raised = meter
            .u64_counter("errors.count")
            .with_description("Total number of errors encountered during operation")
            .build();

        Self {
            tasks_fetched,
            tasks_published,
            empty_responses,
            fetch_requests,
            publish_batches,
            fetch_errors,
            publish_errors,
            health_check_errors,
            error_raised,
            attributes,
        }
    }

    // -- Task operation methods

    /// Records the number of tasks fetched from Amplifier API.
    pub(crate) fn record_tasks_fetched(&self, count: u64) {
        self.tasks_fetched.add(count, &self.attributes);
    }

    /// Records the number of tasks published to the queue.
    pub(crate) fn record_tasks_published(&self, count: u64) {
        self.tasks_published.add(count, &self.attributes);
    }

    /// Records an empty response from Amplifier API.
    pub(crate) fn record_empty_response(&self) {
        self.empty_responses.add(1, &self.attributes);
    }

    /// Records a fetch request made to Amplifier API.
    pub(crate) fn record_fetch_request(&self) {
        self.fetch_requests.add(1, &self.attributes);
    }

    /// Records a batch published to the queue.
    pub(crate) fn record_publish_batch(&self) {
        self.publish_batches.add(1, &self.attributes);
    }

    // -- Error tracking methods

    /// Records an error during task fetching.
    pub(crate) fn record_fetch_error(&self) {
        self.fetch_errors.add(1, &self.attributes);
        self.error_raised.add(1, &self.attributes);
    }

    /// Records an error during task publishing.
    pub(crate) fn record_publish_error(&self) {
        self.publish_errors.add(1, &self.attributes);
        self.error_raised.add(1, &self.attributes);
    }

    /// Records a health check error.
    pub(crate) fn record_health_check_error(&self) {
        self.health_check_errors.add(1, &self.attributes);
        self.error_raised.add(1, &self.attributes);
    }

    /// Records a general error (for compatibility with `SimpleMetrics`).
    pub(crate) fn record_error(&self) {
        self.error_raised.add(1, &self.attributes);
    }
}
