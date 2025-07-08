resource "google_service_account" "tasks_publisher_iam" {
  account_id   = "tasks-pub-${var.axelar_implementation}"
  display_name = "Service account for tasks publisher"
}

resource "google_service_account" "tasks_subscriber_iam" {
  account_id   = "tasks-sub-${var.axelar_implementation}"
  display_name = "Service account for tasks subscriber"
}

resource "google_service_account" "events_publisher_iam" {
  account_id   = "events-pub-${var.axelar_implementation}"
  display_name = "Service account for events publisher"
}

resource "google_service_account" "events_subscriber_iam" {
  account_id   = "events-sub-${var.axelar_implementation}"
  display_name = "Service account for tasks subscriber"
}

