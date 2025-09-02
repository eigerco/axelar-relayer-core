pub(crate) async fn process_healthcheck(
    config: crate::config::Config,
    clock: quanta::Clock,
    client: amplifier_api::AmplifierApiClient,
) -> eyre::Result<()> {
    tracing::info!(interval =? config.healthcheck_interval, "spawned");

    let mut healthcheck_current_failures = 0_usize;
    let mut interval = tokio::time::interval(config.healthcheck_interval);
    loop {
        interval.tick().await;
        let t1 = clock.recent();
        let healthceck_succeeded = client.health_check().await.is_ok();
        let t2 = clock.recent();
        let delta = t2.saturating_duration_since(t1);
        tracing::info!(execution_duration = ?delta, "healthcheck duration");

        // check if amplifier api is unreachable for a long time already
        let invalid_healthchecks_before_shutdown = config.invalid_healthchecks_before_shutdown;

        if healthceck_succeeded {
            healthcheck_current_failures = 0;
            continue;
        }

        healthcheck_current_failures = healthcheck_current_failures.saturating_add(1_usize);
        if healthcheck_current_failures >= invalid_healthchecks_before_shutdown {
            eyre::bail!("cannot reach amplifier API");
        }
    }
}
