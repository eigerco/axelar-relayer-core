use core::task::Poll;

use futures::StreamExt as _;
use futures::stream::FusedStream as _;
use tokio::task::JoinSet;

use super::component::{AmplifierCommand, CommandReceiver};
use super::config::Config;

pub(crate) async fn process(
    config: Config,
    mut receiver: CommandReceiver,
    client: amplifier_api::AmplifierApiClient,
) -> eyre::Result<()> {
    tracing::info!("spawned");

    let mut join_set = JoinSet::<eyre::Result<()>>::new();
    let mut task_stream = futures::stream::poll_fn(move |cx| {
        // check if we have new requests to add to the join set
        match receiver.poll_next_unpin(cx) {
            Poll::Ready(Some(command)) => {
                // spawn the command on the joinset, returning the error
                tracing::info!(?command, "sending message to amplifier api");
                let res = internal(command, &client, &config.chain, &mut join_set);

                cx.waker().wake_by_ref();
                return Poll::Ready(Some(Ok(res)));
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
                    return Poll::Ready(None);
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
    client: &amplifier_api::AmplifierApiClient,
    chain: &str,
    join_set: &mut JoinSet<eyre::Result<()>>,
) -> Result<(), eyre::Error> {
    match command {
        AmplifierCommand::PublishEvents(events) => {
            join_set.spawn({
                let client = client.clone();
                let chain = chain.to_string();
                async move {
                    client
                        .publish_events(&chain, &events)
                        .await
                        .map(|_| ())
                        .map_err(Into::into)
                }
            });
        }
    }

    Ok(())
}
