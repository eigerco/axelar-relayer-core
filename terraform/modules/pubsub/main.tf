locals {
  amplifier_events_topic     = "${var.axelar_implementation}-amplifier-events"
  amplifier_events_sub       = "${var.axelar_implementation}-amplifier-events-sub"
  amplifier_events_dlq_topic = "${var.axelar_implementation}-amplifier-events-dlq"
  amplifier_tasks_topic      = "${var.axelar_implementation}-amplifier-tasks"
  amplifier_tasks_sub        = "${var.axelar_implementation}-amplifier-tasks-sub"
  amplifier_tasks_dlq_topic  = "${var.axelar_implementation}-amplifier-tasks-dlq"
}

resource "google_pubsub_topic" "amplifier_events" {
  name = local.amplifier_events_topic

  message_storage_policy {
    allowed_persistence_regions = var.allowed_persistence_regions
  }

  message_retention_duration = var.message_retention_duration

  labels = var.default_labels
}

resource "google_pubsub_topic" "amplifier_tasks" {
  name = local.amplifier_tasks_topic

  message_storage_policy {
    allowed_persistence_regions = var.allowed_persistence_regions
  }

  message_retention_duration = var.message_retention_duration

  labels = var.default_labels
}

resource "google_pubsub_topic" "amplifier_events_dlq" {
  name = local.amplifier_events_dlq_topic

  message_storage_policy {
    allowed_persistence_regions = var.allowed_persistence_regions
  }

  # Use DLQ retention duration from config
  message_retention_duration = var.dlq_message_retention_duration

  # Add labels if provided
  labels = var.default_labels
}

resource "google_pubsub_topic" "amplifier_tasks_dlq" {
  name = local.amplifier_tasks_dlq_topic

  message_storage_policy {
    allowed_persistence_regions = var.allowed_persistence_regions
  }

  # Use DLQ retention duration from config
  message_retention_duration = var.dlq_message_retention_duration

  # Add labels if provided
  labels = var.default_labels
}

resource "google_pubsub_subscription" "amplifier_events_sub" {
  name  = local.amplifier_events_sub
  topic = google_pubsub_topic.amplifier_events.name

  enable_exactly_once_delivery = true
  ack_deadline_seconds         = var.ack_deadline_seconds

  retry_policy {
    minimum_backoff = var.retry_policy.minimum_backoff
    maximum_backoff = var.retry_policy.maximum_backoff
  }

  dead_letter_policy {
    # Fixed to reference events DLQ instead of tasks DLQ
    dead_letter_topic     = google_pubsub_topic.amplifier_events_dlq.id
    max_delivery_attempts = var.max_delivery_attempts
  }

  expiration_policy {
    ttl = var.expiration_ttl
  }

  # Add labels if provided
  labels = var.default_labels
}

resource "google_pubsub_subscription" "amplifier_tasks_sub" {
  name  = local.amplifier_tasks_sub
  topic = google_pubsub_topic.amplifier_tasks.name

  enable_exactly_once_delivery = true
  ack_deadline_seconds         = var.ack_deadline_seconds

  retry_policy {
    minimum_backoff = var.retry_policy.minimum_backoff
    maximum_backoff = var.retry_policy.maximum_backoff
  }

  dead_letter_policy {
    dead_letter_topic     = google_pubsub_topic.amplifier_tasks_dlq.id
    max_delivery_attempts = var.max_delivery_attempts
  }

  expiration_policy {
    ttl = var.expiration_ttl
  }

  # Add labels if provided
  labels = var.default_labels
}

data "google_iam_policy" "pubsub_publisher" {
  binding {
    role = "roles/pubsub.publisher"
    members = [
      "serviceAccount:${var.publisher_service_account_email}",
    ]
  }
}

data "google_iam_policy" "pubsub_subscriber" {
  binding {
    role = "roles/pubsub.subscriber"
    members = [
      "serviceAccount:${var.subscriber_service_account_email}",
    ]
  }
}

resource "google_pubsub_topic_iam_policy" "tasks_publish" {
  topic       = google_pubsub_topic.amplifier_tasks.name
  policy_data = data.google_iam_policy.pubsub_publisher.policy_data
}

resource "google_pubsub_topic_iam_policy" "tasks_subscribe" {
  topic       = google_pubsub_topic.amplifier_tasks.name
  policy_data = data.google_iam_policy.pubsub_subscriber.policy_data
}

resource "google_pubsub_topic_iam_policy" "events_publish" {
  topic       = google_pubsub_topic.amplifier_events.name
  policy_data = data.google_iam_policy.pubsub_subscriber.policy_data
}

resource "google_pubsub_topic_iam_policy" "events_subscribe" {
  topic       = google_pubsub_topic.amplifier_events.name
  policy_data = data.google_iam_policy.pubsub_publisher.policy_data
}
