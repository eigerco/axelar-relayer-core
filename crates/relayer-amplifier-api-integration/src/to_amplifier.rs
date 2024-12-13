use core::task::Poll;

use amplifier_api::requests::{self, WithTrailingSlash};
use amplifier_api::types::{ErrorResponse, PublishEventsResult};
use amplifier_api::AmplifierRequest;
use futures::stream::FusedStream as _;
use futures::StreamExt as _;
use tokio::task::JoinSet;
use tracing::{info_span, Instrument as _};

use super::component::{AmplifierCommand, CommandReceiver};
use super::config::Config;

pub(crate) async fn process(
    config: Config,
    mut receiver: CommandReceiver,
    client: amplifier_api::AmplifierApiClient,
) -> eyre::Result<()> {
    tracing::info!("spawned");

    let mut join_set = JoinSet::<eyre::Result<()>>::new();
    let chain_with_trailing_slash = WithTrailingSlash::new(config.chain.clone());
    let mut task_stream = futures::stream::poll_fn(move |cx| {
        // check if we have new requests to add to the join set
        match receiver.poll_next_unpin(cx) {
            Poll::Ready(Some(command)) => {
                // spawn the command on the joinset, returning the error
                tracing::info!(?command, "sending message to amplifier api");
                let res = internal(command, &chain_with_trailing_slash, &client, &mut join_set);

                cx.waker().wake_by_ref();
                return Poll::Ready(Some(Ok(res)))
            }
            Poll::Pending => (),
            Poll::Ready(None) => {
                tracing::error!("receiver channel closed");
                join_set.abort_all();
            }
        }
        // check if any background tasks are done
        match join_set.poll_join_next(cx) {
            Poll::Ready(Some(res)) => Poll::Ready(Some(res)),
            // join set returns `Poll::Ready(None)` when it's empty
            Poll::Ready(None) => {
                if receiver.is_terminated() {
                    return Poll::Ready(None)
                }
                Poll::Pending
            }
            Poll::Pending => Poll::Pending,
        }
    });

    while let Some(task_result) = task_stream.next().await {
        let Ok(res) = task_result else {
            tracing::error!(?task_result, "background task panicked");
            continue;
        };
        let Err(err) = res else {
            continue;
        };

        tracing::error!(?err, "background task returned an error");
    }

    eyre::bail!("fatal error when processing messages from amplifier")
}

pub(crate) fn internal(
    command: AmplifierCommand,
    chain_with_trailing_slash: &WithTrailingSlash,
    client: &amplifier_api::AmplifierApiClient,
    join_set: &mut JoinSet<eyre::Result<()>>,
) -> Result<(), eyre::Error> {
    match command {
        AmplifierCommand::PublishEvents(events) => {
            let request = requests::PostEvents::builder()
                .payload(&events)
                .chain(chain_with_trailing_slash)
                .build();
            let request = client.build_request(&request)?;
            join_set.spawn(
                process_publish_events_request(request)
                    .instrument(info_span!("publish events"))
                    .in_current_span(),
            );
        }
    };

    Ok(())
}

async fn process_publish_events_request(
    request: AmplifierRequest<PublishEventsResult, ErrorResponse>,
) -> eyre::Result<()> {
    let res = request.execute().await?;
    let res = res.json().await??;
    for item in res.results {
        use amplifier_api::types::PublishEventResultItem::{Accepted, Error};
        match item {
            Accepted(accepted) => {
                tracing::info!(?accepted, "event registered");
                // no op
            }
            Error(err) => {
                tracing::warn!(?err, "could not publish event");
                // todo handle with retries
            }
        }
    }
    Ok(())
}
