resource "google_kms_key_ring" "amplifier_api" {
  name     = "amplifier_api_keyring"
  location = "global"
}

resource "google_kms_crypto_key" "amplifier_api_signing_key" {
  name     = "amplifier_api_signing_key"
  key_ring = google_kms_key_ring.amplifier_api.id
  purpose  = "ASYMMETRIC_SIGN"

  version_template {
    protection_level = var.protection_level
    algorithm        = "RSA_SIGN_PKCS1_4096_SHA256"
  }
}

data "google_iam_policy" "amplifier_sign_policy" {
  binding {
    role = "roles/cloudkms.signer"

    members = [
    "serviceAccount:${var.events_publisher_service_account_email}",
    "serviceAccount:${var.tasks_subscriber_service_account_email}"
    ]
  }
}

resource "google_kms_crypto_key_iam_policy" "amplifier_sign_policy" {
  crypto_key_id = google_kms_crypto_key.amplifier_api_signing_key.id
  policy_data = data.google_iam_policy.amplifier_sign_policy.policy_data
}

