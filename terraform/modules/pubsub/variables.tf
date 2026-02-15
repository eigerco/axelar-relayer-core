variable "axelar_implementation" {
  type        = string
  description = "Name of axelar project/specific implementation that will be appended to account names"
}

variable "ack_deadline_seconds" {
  description = "The maximum time after a subscriber receives a message before the subscriber should acknowledge the message"
  type        = number
  default     = 20
}

variable "retry_policy" {
  description = "The retry policy for the Pub/Sub subscription"
  type = object({
    minimum_backoff = string
    maximum_backoff = string
  })
  default = {
    minimum_backoff = "10s"
    maximum_backoff = "600s"
  }
}

variable "max_delivery_attempts" {
  description = "The maximum number of delivery attempts for any message"
  type        = number
  default     = 5
}

variable "expiration_ttl" {
  description = "Specifies the TTL for messages"
  type        = string
  default     = "2678400s" # 31 days
}

variable "message_retention_duration" {
  description = "How long to retain undelivered messages"
  type        = string
  default     = "2678400s" # 31 days
}

variable "dlq_message_retention_duration" {
  description = "How long to retain undelivered messages in the DLQ"
  type        = string
  default     = "2678400s" # 31 days
}

variable "allowed_persistence_regions" {
  description = "List of regions where messages can be stored"
  type        = list(string)
  default     = ["europe-north2"]
}

variable "publisher_service_account_email" {
  type        = string
  description = "The email address of the tasks publisher service account"
}

variable "subscriber_service_account_email" {
  type        = string
  description = "The email address of the tasks subscriber service account"
}

variable "default_labels" {
  description = "Default labels to apply to all resources"
  type        = map(string)
  default     = {}
}
