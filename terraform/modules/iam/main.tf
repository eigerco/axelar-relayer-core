resource "google_service_account" "tasks_publisher_iam" {
  account_id   = "tasks-publisher-${var.axelar_implementation}"
  display_name = "Service account for tasks publisher"
}

resource "google_service_account" "tasks_subscriber_iam" {
  account_id   = "tasks-subscriber-${var.axelar_implementation}"
  display_name = "Service account for tasks subscriber"
}

resource "google_service_account" "events_publisher_iam" {
  account_id   = "events-publisher-${var.axelar_implementation}"
  display_name = "Service account for events publisher"
}

resource "google_service_account" "events_subscriber_iam" {
  account_id   = "events-subscriber-${var.axelar_implementation}"
  display_name = "Service account for tasks subscriber"
}

