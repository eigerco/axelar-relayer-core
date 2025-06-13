# OpenTofu/Terraform Infrastructure

This directory contains Terraform modules for provisioning the Google Cloud Platform (GCP) infrastructure required by the Axelar Relayer system.

## Overview

The Terraform configuration sets up a complete messaging and processing infrastructure on GCP, including:
- Pub/Sub topics and subscriptions for event-driven communication
- Service accounts with appropriate IAM permissions
- Key Management Service (KMS) for cryptographic operations
- Memorystore (Redis) for caching and state management
- Google Kubernetes Engine (GKE) for container orchestration

## Modules

### [IAM Module](./modules/iam/)
Manages Google Cloud service accounts and their permissions:
- `tasks-publisher` - Service account for publishing tasks to Pub/Sub
- `tasks-subscriber` - Service account for subscribing to task messages
- `events-publisher` - Service account for publishing events to Pub/Sub
- `events-subscriber` - Service account for subscribing to event messages

### [K8s Module](./modules/k8s/)
Provisions Google Kubernetes Engine infrastructure:
- GKE cluster with configurable location and node count
- Kubernetes namespace for the application
- Node pools with appropriate OAuth scopes for GCP services

**Note**: This module is marked as TODO and needs to be imported into your environment.

### [KMS Module](./modules/kms/)
Sets up Google Cloud Key Management Service:
- KMS key ring named `amplifier_api_keyring`
- Asymmetric signing key (RSA 4096-bit SHA256) for the Amplifier API
- IAM policies granting signing permissions to relevant service accounts

### [Memorystore Module](./modules/memorystore/)
Provisions Google Cloud Memorystore (Redis):
- Redis instance with 1GB memory
- Persistence configuration with RDB snapshots every 12 hours
- Used for caching and key-value storage

### [Pub/Sub Module](./modules/pubsub/)
Creates the messaging infrastructure:

**Topics:**
- `amplifier-events` - Main topic for event messages
- `amplifier-tasks` - Main topic for task messages
- `amplifier-events-dlq` - Dead letter queue for failed event processing
- `amplifier-tasks-dlq` - Dead letter queue for failed task processing

**Subscriptions:**
- `amplifier-events-sub` - Subscription to the events topic
- `amplifier-tasks-sub` - Subscription to the tasks topic

**Features:**
- Exactly-once delivery enabled
- Configurable retry policies with exponential backoff
- Dead letter queues for handling failed messages
- Regional message persistence controls
- 31-day message retention (configurable)

## Prerequisites

1. **Google Cloud Project**: You need an active GCP project with billing enabled
2. **Terraform**: Install Terraform (version >= 1.0)
3. **GCP Authentication**: Configure authentication using one of:
   - `gcloud auth application-default login`
   - Service account key file
   - Workload Identity Federation

## Usage

1. **Initialize Terraform**:
   ```bash
   tofu init
   ```

2. **Create a variables file** (e.g., `terraform.tfvars`):
   ```hcl
   project_id = "your-gcp-project-id"
   region     = "us-central1"
   # Add other required variables
   ```

3. **Plan the deployment**:
   ```bash
   tofu plan
   ```

4. **Apply the configuration**:
   ```bash
   tofu apply
   ```

## Module Dependencies

The modules have the following dependencies:
1. IAM module should be deployed first (creates service accounts)
2. KMS module depends on IAM (grants permissions to service accounts)
3. Pub/Sub module depends on IAM (grants topic/subscription permissions)
4. Memorystore and K8s modules can be deployed independently

## Important Notes

- The K8s module is marked as TODO and requires additional implementation
- Memorystore IAM bindings are commented out pending K8s implementation
- All resources are created with specific naming conventions for the Axelar relayer system
- Dead letter queues are configured with a maximum of 5 delivery attempts

## Security Considerations

- Service accounts follow the principle of least privilege
- KMS keys are used for cryptographic signing operations
- Pub/Sub subscriptions use exactly-once delivery for message reliability
- All resources are configured with appropriate IAM policies

## Troubleshooting

If you encounter issues:
1. Verify your GCP project has the required APIs enabled
2. Check that your authentication credentials have sufficient permissions
3. Review the Terraform state file for any inconsistencies
4. Use `tofu destroy` carefully to clean up resources

