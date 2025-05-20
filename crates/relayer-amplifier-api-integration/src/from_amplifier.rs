use core::task::Poll;

use amplifier_api::AmplifierRequest;
use amplifier_api::requests::{self, WithTrailingSlash};
use amplifier_api::types::{ErrorResponse, GetTasksResult};
use futures::SinkExt as _;
use futures::stream::StreamExt as _;
use relayer_amplifier_state::State;
use tokio::task::JoinSet;
use tokio_stream::wrappers::IntervalStream;

use crate::component::AmplifierTaskSender;
use crate::config::Config;

// process incoming messages (aka `tasks`) coming in form Amplifier API
// 1. periodically check if we have new tasks for processing
// 2. if we do, try to act on them; spawning handlers concurrently
pub(crate) async fn process<S>(
    config: Config,
    client: amplifier_api::AmplifierApiClient,
    fan_out_sender: AmplifierTaskSender,
    state: S,
) -> eyre::Result<()>
where
    S: State,
{
    tracing::info!(poll_interval =? config.get_chains_poll_interval, "spawned");

    // Trailing slash is significant when making the API calls!
    let chain_with_trailing_slash = WithTrailingSlash::new(config.chain.clone());
    let mut join_set = JoinSet::<eyre::Result<()>>::new();

    let mut interval_stream = IntervalStream::new({
        let mut interval = tokio::time::interval(config.get_chains_poll_interval);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval
    });

    // Upon startup we want to continue from the latest processed item
    if let Some(task_item_id) = state.latest_processed_task_id() {
        state.set_latest_queried_task_id(task_item_id)?;
    }

    let mut task_stream = futures::stream::poll_fn(move |cx| {
        // periodically query the API for new tasks but only if the downstream processor is ready to
        // accept
        match interval_stream.poll_next_unpin(cx) {
            Poll::Ready(Some(_res)) => {
                let res = internal(
                    &config,
                    &chain_with_trailing_slash,
                    &client,
                    fan_out_sender.clone(),
                    &mut join_set,
                    state.clone(),
                );
                // in case we were awoken by join_set being ready, let's re-run this function,
                // while returning the result of `internal`.
                cx.waker().wake_by_ref();
                return Poll::Ready(Some(Ok(res)));
            }
            Poll::Pending => (),
            Poll::Ready(None) => {
                tracing::error!("interval stream closed");
                join_set.abort_all();
            }
        }

        // check if any background tasks are done
        match join_set.poll_join_next(cx) {
            Poll::Ready(Some(res)) => Poll::Ready(Some(res)),
            // join set returns `Poll::Ready(None)` when it's empty
            Poll::Ready(None) | Poll::Pending => Poll::Pending,
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

pub(crate) fn internal<S>(
    config: &Config,
    chain_with_trailing_slash: &WithTrailingSlash,
    client: &amplifier_api::AmplifierApiClient,
    fan_out_sender: AmplifierTaskSender,
    to_join_set: &mut JoinSet<eyre::Result<()>>,
    state: S,
) -> eyre::Result<()>
where
    S: State,
{
    if !fan_out_sender.is_empty() {
        // the downstream client is still processing the events, don't send any new ones
        return Ok(());
    }
    let latest_processed_task = state.latest_processed_task_id();
    let latest_queried_task = state.latest_queried_task_id();
    if latest_processed_task != latest_queried_task {
        tracing::trace!("downstream processor still processing the last batch");
        return Ok(());
    }
    tracing::trace!(?latest_processed_task, "latest task to query");
    let request = requests::GetChains::builder()
        .chain(chain_with_trailing_slash)
        .limit(config.get_chains_limit)
        .after(latest_processed_task)
        .build();
    let request = client.build_request(&request)?;
    to_join_set.spawn(process_task_request(request, fan_out_sender, state));

    Ok(())
}

async fn process_task_request<S: State>(
    request: AmplifierRequest<GetTasksResult, ErrorResponse>,
    mut fan_out_sender: AmplifierTaskSender,
    state: S,
) -> eyre::Result<()> {
    let res = request.execute().await?;
    let res = res.json().await??;
    let Some(last_task_item_id) = res.tasks.last().map(|x| x.id.clone()) else {
        return Ok(());
    };
    tracing::info!(
        new_tasks = ?res.tasks.len(),
        latest_queried_task_id =? last_task_item_id,
        "received new tasks"
    );
    let mut iter = futures::stream::iter(res.tasks.into_iter().map(Ok));
    fan_out_sender.send_all(&mut iter).await?;
    state.set_latest_queried_task_id(last_task_item_id)?;
    Ok(())
}
