variable "axelar_implementation" {
  type        = string
  description = "Name of axelar project/specific implementation that will be appended to account names"
}

variable "protection_level" {
  type        = string
  description = "Protection level of signing key"
}

variable "events_publisher_service_account_email" {
  type        = string
  description = "The email address of the events publisher service account"
}

variable "tasks_subscriber_service_account_email" {
  type        = string
  description = "The email address of the tasks subscriber service account"
}
