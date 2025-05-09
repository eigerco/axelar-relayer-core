variable "cluster_name" {
  type        = string
  description = "Name of the GKE cluster"
}

variable "cluster_location" {
  type        = string
  description = "The location of the GKE cluster"
}

variable "machine_type" {
  type        = string
  description = "Node machine type"
}
