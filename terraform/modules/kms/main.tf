resource "google_kms_key_ring" "amplifier_api" {
  name     = "amplifier_api_keyring"
  location = "global"
}

resource "google_kms_crypto_key" "amplifier_api_sign_key" {
  name     = "amplifier_api_sign_key"
  key_ring = google_kms_key_ring.amplifier_api.id
  purpose  = "ASYMMETRIC_SIGN"

  version_template {
    protection_level = var.protection_level
    algorithm        = "RSA_SIGN_PKCS1_4096_SHA256"
  }
}

resource "google_kms_crypto_key_iam_member" "key_signer" {
  crypto_key_id = google_kms_crypto_key.amplifier_api_sign_key.id
  role          = "roles/cloudkms.signer"
  members = [
    "serviceAccount:${var.events_publisher_service_account_email}"
    "serviceAccount:${var.tasks_subscriber_service_account_email}"
  ]
}
