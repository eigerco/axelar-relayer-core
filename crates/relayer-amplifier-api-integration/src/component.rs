use core::future::Future;
use core::pin::Pin;

use amplifier_api::types::{PublishEventsRequest, TaskItem};
use futures_concurrency::future::FutureExt as _;
use quanta::Upkeep;
use relayer_amplifier_state::State;
use serde::{Deserialize, Serialize};
use tracing::{Instrument as _, info_span};

use crate::{config, from_amplifier, healthcheck, to_amplifier};

/// A valid command that the Amplifier component can act upon
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AmplifierCommand {
    /// Publish events to the Amplifier API
    PublishEvents(PublishEventsRequest),
}

pub(crate) type CommandReceiver = futures::channel::mpsc::UnboundedReceiver<AmplifierCommand>;
pub(crate) type AmplifierTaskSender = futures::channel::mpsc::UnboundedSender<TaskItem>;

/// The core Amplifier API abstraction.
///
/// Internally, it spawns processes for:
/// - monitoring the liveliness of the Amplifier API (via healthcheck)
/// - listening for new tasks coming form Amplifeir API (Listener subprocess)
/// - sending events to the Amplifier API (Subscriber subprocess)
#[derive(Debug)]
pub struct Amplifier<S: State> {
    config: config::Config,
    receiver: CommandReceiver,
    sender: AmplifierTaskSender,
    state: S,
}

impl core::fmt::Display for AmplifierCommand {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::PublishEvents(request) => {
                writeln!(f, "AmplifierCommand::PublishEvents:")?;
                writeln!(f, "  request:")?;
                for line in request.to_string().lines() {
                    writeln!(f, "    {line}")?;
                }
            }
        }
        Ok(())
    }
}

/// Utility client used for communicating with the `Amplifier` instance
#[derive(Debug, Clone)]
pub struct AmplifierCommandClient {
    /// send commands to the `Amplifier` instance
    pub sender: futures::channel::mpsc::UnboundedSender<AmplifierCommand>,
}

/// Utility client used for getting data from the `Amplifier` instance
#[derive(Debug)]
pub struct AmplifierTaskReceiver {
    /// send commands to the `Amplifier` instance
    pub receiver: futures::channel::mpsc::UnboundedReceiver<TaskItem>,
}

impl<S> relayer_engine::RelayerComponent for Amplifier<S>
where
    S: State,
{
    fn process(self: Box<Self>) -> Pin<Box<dyn Future<Output = eyre::Result<()>> + Send>> {
        use futures::FutureExt as _;

        self.process_internal().boxed()
    }
}

impl<S> Amplifier<S>
where
    S: State,
{
    /// Instantiate a new Amplifier using the pre-configured configuration.
    ///
    /// The returned variable also returns a helper client that encompasses ways to communicate with
    /// the underlying Amplifier instance.
    #[must_use]
    pub fn new(
        config: config::Config,
        state: S,
    ) -> (Self, AmplifierCommandClient, AmplifierTaskReceiver) {
        let (command_tx, command_rx) = futures::channel::mpsc::unbounded();
        let (task_tx, task_rx) = futures::channel::mpsc::unbounded();
        let this = Self {
            config,
            state,
            sender: task_tx,
            receiver: command_rx,
        };
        let client = AmplifierCommandClient { sender: command_tx };
        let task_client = AmplifierTaskReceiver { receiver: task_rx };
        (this, client, task_client)
    }

    #[tracing::instrument(skip_all, name = "Amplifier")]
    pub(crate) async fn process_internal(self) -> eyre::Result<()> {
        let config = self.config.clone();
        let client = amplifier_api::AmplifierApiClient::new(
            self.config.url.clone(),
            amplifier_api::TlsType::Certificate(Box::new(
                self.config
                    .identity
                    .ok_or_else(|| eyre::Report::msg("identity cert+key not set"))?,
            )),
        )?;
        let clock = get_clock()?;

        // spawn tasks
        let healthcheck = healthcheck::process_healthcheck(config.clone(), clock, client.clone())
            .instrument(info_span!("healthcheck"))
            .in_current_span();
        let to_amplifier_msgs =
            to_amplifier::process(config.clone(), self.receiver, client.clone())
                .instrument(info_span!("to amplifier"))
                .in_current_span();
        let from_amplifier_msgs =
            from_amplifier::process(config, client.clone(), self.sender.clone(), self.state)
                .instrument(info_span!("from amplifier"))
                .in_current_span();

        // await tasks until one of them exits (fatal)
        healthcheck
            .race(to_amplifier_msgs)
            .race(from_amplifier_msgs)
            .await?;
        eyre::bail!("listener crashed");
    }
}

pub(crate) fn get_clock() -> eyre::Result<quanta::Clock> {
    static CLOCK_UPKEEP_HANDLE: std::sync::OnceLock<quanta::Handle> =
        std::sync::OnceLock::<quanta::Handle>::new();
    let tickrate = core::time::Duration::from_millis(10);
    if let Err(quanta::Error::FailedToSpawnUpkeepThread(_)) =
        Upkeep::new(tickrate).start().map(|x| {
            CLOCK_UPKEEP_HANDLE
                .set(x)
                .expect("could not set upkeep handle");
        })
    {
        eyre::bail!("could not spawn clock upkeep thread");
    }
    let clock = quanta::Clock::new();
    Ok(clock)
}
