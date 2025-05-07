//! Types for Axelar Amplifier API
//! Contsructed form the following API spec [link](https://github.com/axelarnetwork/axelar-eds-mirror/blob/main/oapi/gmp/schema.yaml)

use core::fmt::{Display, Formatter};

pub use big_int::BigInt;
use borsh::{BorshDeserialize, BorshSerialize};
use chrono::{DateTime, Utc};
pub use id::*;
use infrastructure::interfaces::publisher::QueueMsgId;
use serde::{Deserialize, Serialize};
use typed_builder::TypedBuilder;
pub use {bnum, uuid};

/// Represents an address as a non-empty string.
pub type Address = String;

/// Newtypes for different types of IDs so we don't mix them up in the future
mod id {

    use borsh::{BorshDeserialize, BorshSerialize};

    use super::*;

    /// `NewType` for tracking transaction ids
    ///
    /// for in-depth docs reference [this document](https://bright-ambert-2bd.notion.site/Amplifier-GMP-API-EXTERNAL-911e740b570b4017826c854338b906c8#e8a7398607bd496eb0b8e95e887d6574)
    #[derive(
        Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
    )]
    pub struct TxId(pub String);

    /// `NewType` for tracking token ids
    ///
    /// for in-depth docs reference [this document](https://bright-ambert-2bd.notion.site/Amplifier-GMP-API-EXTERNAL-911e740b570b4017826c854338b906c8#e8a7398607bd496eb0b8e95e887d6574)
    #[derive(
        Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
    )]
    pub struct TokenId(pub String);

    /// Indicates a type in format of `TxHash-LogIndex`
    ///
    /// TxHash-LogIndex. for in-depth docs reference [this document](https://bright-ambert-2bd.notion.site/Amplifier-GMP-API-EXTERNAL-911e740b570b4017826c854338b906c8#e8a7398607bd496eb0b8e95e887d6574)
    #[derive(
        Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
    )]
    pub struct TxEvent(pub String);
    impl TxEvent {
        /// construct a new event identifier from the tx hash and the log index
        #[must_use]
        pub fn new(tx_hash: &str, log_index: usize) -> Self {
            Self(format!("{tx_hash}-{log_index}"))
        }

        /// Construct a new event id for a [`CannotExecuteMessageEvent`]
        #[must_use]
        pub fn cannot_execute_task_event_id(task_item_id: &TaskItemId) -> Self {
            Self(format!("cannot-execute-task-v2-{}", task_item_id.0))
        }

        /// Construct a new event id for a [`MessageExecutedEvent`] with
        /// [`MessageExecutionStatus::Reverted`]
        #[must_use]
        pub fn tx_reverted_event_id(tx_hash: &str) -> Self {
            Self(format!("tx-reverted-{tx_hash}"))
        }
    }

    impl core::fmt::Display for TxEvent {
        fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    /// `NewType` for tracking message ids
    pub type MessageId = TxEvent;

    /// Id of chain `NativeGasPaidForContractCall`/ `NativeGasAdded` event in format
    pub type EventId = TxEvent;

    /// `NewType` for tracking task ids
    ///
    /// for in-depth docs reference [this document](https://bright-ambert-2bd.notion.site/Amplifier-GMP-API-EXTERNAL-911e740b570b4017826c854338b906c8#e8a7398607bd496eb0b8e95e887d6574)
    #[derive(
        Clone, Serialize, Deserialize, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize,
    )]
    pub struct TaskItemId(pub uuid::Uuid);

    impl core::fmt::Display for TaskItemId {
        fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
            write!(f, "{}", self.0)
        }
    }

    /// `NewType` for tracking command ids.
    ///
    /// for in-depth docs reference [this document](https://bright-ambert-2bd.notion.site/Amplifier-GMP-API-EXTERNAL-911e740b570b4017826c854338b906c8#e8a7398607bd496eb0b8e95e887d6574)
    #[derive(
        Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
    )]
    pub struct CommandId(pub String);

    /// `NewType` for tracking request ids.
    ///
    /// for in-depth docs reference [this document](https://bright-ambert-2bd.notion.site/Amplifier-GMP-API-EXTERNAL-911e740b570b4017826c854338b906c8#e8a7398607bd496eb0b8e95e887d6574)
    #[derive(
        Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
    )]
    pub struct RequestId(pub String);
    #[expect(
        clippy::min_ident_chars,
        reason = "don't rename variable names from the trait"
    )]
    impl core::fmt::Display for RequestId {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.write_str(self.0.as_str())
        }
    }
}

mod serde_utils {
    use base64::prelude::*;
    use serde::{Deserialize as _, Deserializer, Serializer};

    pub(crate) fn base64_decode<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw_string = String::deserialize(deserializer)?;
        let bytes = BASE64_STANDARD
            .decode(raw_string)
            .inspect_err(|err| {
                tracing::error!(?err, "cannot parse base64 data");
            })
            .map_err(serde::de::Error::custom)?;
        Ok(bytes)
    }

    pub(crate) fn base64_encode<S>(data: &Vec<u8>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let encoded = BASE64_STANDARD.encode(data);
        serializer.serialize_str(&encoded)
    }
}

/// Enumeration of reasons why a message cannot be executed.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CannotExecuteMessageReason {
    /// Not enough gas to execute the message
    InsufficientGas,
    /// Other generic error
    Error,
}

impl Display for CannotExecuteMessageReason {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InsufficientGas => write!(f, "insufficient gas"),
            Self::Error => write!(f, "other error"),
        }
    }
}

/// Enumeration of message execution statuses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MessageExecutionStatus {
    /// Message executed successfully
    Successful,
    /// Message reverted
    Reverted,
}

impl Display for MessageExecutionStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Successful => write!(f, "successful"),
            Self::Reverted => write!(f, "reverted"),
        }
    }
}

/// Represents metadata associated with an event.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct EventMetadata<T> {
    /// tx id of the underlying event
    #[serde(rename = "txID", skip_serializing_if = "Option::is_none")]
    #[builder(default)]
    pub tx_id: Option<TxId>,
    /// timestamp of the underlying event
    #[serde(rename = "timestamp", skip_serializing_if = "Option::is_none")]
    #[builder(default)]
    #[borsh(
        serialize_with = "crate::util::serialize_option_utc",
        deserialize_with = "crate::util::deserialize_option_utc"
    )]
    pub timestamp: Option<DateTime<Utc>>,
    /// sender address
    #[serde(rename = "fromAddress", skip_serializing_if = "Option::is_none")]
    #[builder(default)]
    pub from_address: Option<Address>,
    /// weather the event is finalized or not
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[builder(default)]
    pub finalized: Option<bool>,
    /// Extra fields that are dependant on the core event
    #[serde(flatten)]
    pub extra: T,
}

impl<T> Display for EventMetadata<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "EventMetadata:")?;
        if let Some(tx_id) = &self.tx_id {
            writeln!(f, "  txID: {}", tx_id.0)?;
        }
        if let Some(timestamp) = &self.timestamp {
            writeln!(f, "  timestamp: {timestamp}")?;
        }
        if let Some(from_address) = &self.from_address {
            writeln!(f, "  fromAddress: {from_address}")?;
        }
        if let Some(finalized) = &self.finalized {
            writeln!(f, "  finalized: {finalized}")?;
        }
        Ok(())
    }
}

/// Specialized metadata for `CallEvent`.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct CallEventMetadata {
    /// the message id that's responsible for the event
    #[serde(rename = "parentMessageID", skip_serializing_if = "Option::is_none")]
    #[builder(default)]
    pub parent_message_id: Option<MessageId>,
}

/// Specialized metadata for `MessageApprovedEvent`.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct MessageApprovedEventMetadata {
    /// The command id that corresponds to the approved message
    #[serde(rename = "commandID", skip_serializing_if = "Option::is_none")]
    #[builder(default)]
    pub command_id: Option<CommandId>,
}

/// Specialized metadata for `MessageExecutedEvent`.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct MessageExecutedEventMetadata {
    /// The command id that corresponds to the executed message
    #[serde(rename = "commandID", skip_serializing_if = "Option::is_none")]
    #[builder(default)]
    pub command_id: Option<CommandId>,
    /// The message
    #[serde(rename = "childMessageIDs", skip_serializing_if = "Option::is_none")]
    #[builder(default)]
    pub child_message_ids: Option<Vec<MessageId>>,
}

/// Specialized metadata for `CannotExecuteMessageEventV2`.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct CannotExecuteMessageEventV2Metadata {
    /// The initiator of the message
    #[serde(rename = "fromAddress", skip_serializing_if = "Option::is_none")]
    #[builder(default)]
    pub from_address: Option<Address>,
    /// timestamp of the event
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(default)]
    #[borsh(
        serialize_with = "crate::util::serialize_option_utc",
        deserialize_with = "crate::util::deserialize_option_utc"
    )]
    pub timestamp: Option<DateTime<Utc>>,
    /// task id
    #[serde(rename = "taskItemID")]
    pub task_item_id: TaskItemId,
}

/// Represents a token amount, possibly with a token ID.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct Token {
    /// indicates that amount is in native token if left blank
    #[serde(rename = "tokenID", skip_serializing_if = "Option::is_none")]
    #[builder(default)]
    pub token_id: Option<TokenId>,
    /// the amount in token’s denominator
    #[borsh(
        serialize_with = "crate::util::serialize_bigint",
        deserialize_with = "crate::util::deserialize_bigint"
    )]
    pub amount: BigInt,
}

impl Display for Token {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match &self.token_id {
            Some(token_id) => write!(f, "{:?} {}", self.amount, token_id.0),
            None => write!(f, "{:?} NativeToken", self.amount),
        }
    }
}

/// Represents a cross-chain message.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct GatewayV2Message {
    /// the message id of a GMP call
    #[serde(rename = "messageID")]
    pub message_id: MessageId,
    /// source chain
    #[serde(rename = "sourceChain")]
    pub source_chain: String,
    /// source address
    #[serde(rename = "sourceAddress")]
    pub source_address: Address,
    /// destination address
    #[serde(rename = "destinationAddress")]
    pub destination_address: Address,
    /// string representation of the payload hash
    #[serde(
        rename = "payloadHash",
        deserialize_with = "serde_utils::base64_decode",
        serialize_with = "serde_utils::base64_encode"
    )]
    pub payload_hash: Vec<u8>,
}

impl Display for GatewayV2Message {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "GatewayV2Message:")?;
        writeln!(f, "  messageID: {}", self.message_id.0)?;
        writeln!(f, "  sourceChain: {}", self.source_chain)?;
        writeln!(f, "  sourceAddress: {}", self.source_address)?;
        writeln!(f, "  destinationAddress: {}", self.destination_address)?;
        Ok(())
    }
}

/// Base struct for events.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct EventBase<T = ()> {
    /// The event id
    #[serde(rename = "eventID")]
    pub event_id: EventId,
    /// Metadata of the event
    #[serde(skip_serializing_if = "Option::is_none")]
    #[builder(default)]
    pub meta: Option<EventMetadata<T>>,
}

impl<T> Display for EventBase<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let output = if let Some(meta) = &self.meta {
            let meta_string = meta.to_string();
            if meta_string.is_empty() {
                format!("eventId - {}", self.event_id.0)
            } else {
                meta_string
                    .lines()
                    .map(|line| format!("  {line}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        } else {
            format!("eventId - {}", self.event_id.0)
        };

        write!(f, "{output}")
    }
}

/// Represents a Gas Credit Event.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct GasCreditEvent {
    /// Event base
    #[serde(flatten)]
    pub base: EventBase,
    /// Message ID
    #[serde(rename = "messageID")]
    pub message_id: MessageId,
    /// the refunder address of the `NativeGasPaidForContractCall`/ `NativeGasAdded` event
    #[serde(rename = "refundAddress")]
    pub refund_address: Address,
    /// payment for the Contract Call
    pub payment: Token,
}

impl Display for GasCreditEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "GasCreditEvent:")?;
        writeln!(f, "  base:")?;
        for line in self.base.to_string().lines() {
            writeln!(f, "    {line}")?;
        }
        writeln!(f, "  messageID: {}", self.message_id.0)?;
        writeln!(f, "  refundAddress: {}", self.refund_address)?;
        writeln!(f, "  payment: {}", self.payment)?;
        Ok(())
    }
}

/// Represents a Gas Refunded Event.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct GasRefundedEvent {
    /// Event base
    #[serde(flatten)]
    pub base: EventBase,
    /// Message ID
    #[serde(rename = "messageID")]
    pub message_id: MessageId,
    /// address of the refund recipient
    #[serde(rename = "recipientAddress")]
    pub recipient_address: Address,
    /// the amount to refund
    #[serde(rename = "refundedAmount")]
    pub refunded_amount: Token,
    /// the cost of the refund transaction
    pub cost: Token,
}

impl Display for GasRefundedEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "GasRefundedEvent:")?;
        writeln!(f, "  base:")?;
        for line in self.base.to_string().lines() {
            writeln!(f, "    {line}")?;
        }
        writeln!(f, "  messageID: {}", self.message_id.0)?;
        writeln!(f, "  recipientAddress: {}", self.recipient_address)?;
        writeln!(f, "  refundedAmount: {}", self.refunded_amount)?;
        writeln!(f, "  cost: {}", self.cost)?;
        Ok(())
    }
}

/// Represents a Call Event.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct CallEvent {
    /// Event base
    #[serde(flatten)]
    pub base: EventBase<CallEventMetadata>,
    /// The cross chain message
    pub message: GatewayV2Message,
    /// Name of the destination chain
    #[serde(rename = "destinationChain")]
    pub destination_chain: String,
    /// string representation of the payload bytes
    #[serde(
        deserialize_with = "serde_utils::base64_decode",
        serialize_with = "serde_utils::base64_encode"
    )]
    pub payload: Vec<u8>,
}

impl Display for CallEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "CallEvent:")?;
        writeln!(f, "  base:")?;
        for line in self.base.to_string().lines() {
            writeln!(f, "    {line}")?;
        }
        writeln!(f, "  message:")?;
        for line in self.message.to_string().lines() {
            writeln!(f, "    {line}")?;
        }
        writeln!(f, "  destinationChain: {}", self.destination_chain)?;
        Ok(())
    }
}
/// Represents a Message Approved Event.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct MessageApprovedEvent {
    /// Event base
    #[serde(flatten)]
    pub base: EventBase<MessageApprovedEventMetadata>,
    /// The cross chain message
    pub message: GatewayV2Message,
    /// the cost of the approval. (#of approvals in transaction / transaction cost)
    pub cost: Token,
}

impl Display for MessageApprovedEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "MessageApprovedEvent:")?;
        writeln!(f, "  base: {}", self.base)?;
        writeln!(f, "  message:")?;
        for line in self.message.to_string().lines() {
            writeln!(f, "    {line}")?;
        }
        writeln!(f, "  cost: {}", self.cost)?;
        Ok(())
    }
}
/// Event that gets emitted upon signer rotatoin
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct SignersRotatedEvent {
    /// Event base
    #[serde(flatten)]
    pub base: EventBase<SignersRotatedMetadata>,
    /// the cost of the approval. (#of approvals in transaction / transaction cost)
    pub cost: Token,
}

impl Display for SignersRotatedEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "SignersRotatedEvent:")?;
        writeln!(f, "  base: {}", self.base)?;
        writeln!(f, "  cost: {}", self.cost)?;
        Ok(())
    }
}
/// Represents extra metadata that can be added to the signers rotated event
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct SignersRotatedMetadata {
    /// The hash of the new signer set
    #[serde(rename = "signerHash")]
    #[serde(
        deserialize_with = "serde_utils::base64_decode",
        serialize_with = "serde_utils::base64_encode"
    )]
    pub signer_hash: Vec<u8>,
    /// The epoch of the new signer set
    pub epoch: u64,
}

/// Represents a Message Executed Event.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct MessageExecutedEvent {
    /// Event base
    #[serde(flatten)]
    pub base: EventBase<MessageExecutedEventMetadata>,
    /// message id
    #[serde(rename = "messageID")]
    pub message_id: MessageId,
    #[serde(rename = "sourceChain")]
    /// source chian
    pub source_chain: String,
    /// message execution status
    pub status: MessageExecutionStatus,
    /// the cost of the transaction containing the execution
    pub cost: Token,
}

impl Display for MessageExecutedEvent {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "MessageExecutedEvent:")?;
        writeln!(f, "  base: {}", self.base)?;
        writeln!(f, "  messageID: {}", self.message_id.0)?;
        writeln!(f, "  sourceChain: {}", self.source_chain)?;
        writeln!(f, "  status: {}", self.status)?;
        writeln!(f, "  cost: {}", self.cost)?;
        Ok(())
    }
}
/// Represents the v2 of Cannot Execute Message Event.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct CannotExecuteMessageEventV2 {
    /// Event base
    #[serde(flatten)]
    pub base: EventBase<CannotExecuteMessageEventV2Metadata>,
    /// the message id of a GMP call
    #[serde(rename = "messageID")]
    pub message_id: MessageId,
    /// source chain
    #[serde(rename = "sourceChain")]
    pub source_chain: String,
    /// failed executioin reason
    pub reason: CannotExecuteMessageReason,
    /// details of the error
    pub details: String,
}

impl Display for CannotExecuteMessageEventV2 {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "CannotExecuteMessageEventV2:")?;
        writeln!(f, "  base: {}", self.base)?;
        writeln!(f, "  messageID: {}", self.message_id.0)?;
        writeln!(f, "  sourceChain: {}", self.source_chain)?;
        writeln!(f, "  reason: {}", self.reason)?;
        writeln!(f, "  details: {}", self.details)?;
        Ok(())
    }
}
/// Represents a generic Event, which can be any of the specific event types.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Event {
    /// gas credit event
    GasCredit(GasCreditEvent),
    /// gas refunded event
    GasRefunded(GasRefundedEvent),
    /// call contract event
    Call(CallEvent),
    /// message approved event
    MessageApproved(MessageApprovedEvent),
    /// message executed event
    MessageExecuted(MessageExecutedEvent),
    /// v2 of cannot execute message event
    #[serde(rename = "CANNOT_EXECUTE_MESSAGE/V2")]
    CannotExecuteMessageV2(CannotExecuteMessageEventV2),
    /// Signers have been rotated
    SignersRotated(SignersRotatedEvent),
}

impl QueueMsgId for Event {
    type MessageId = TxEvent;

    fn id(&self) -> Self::MessageId {
        match self {
            Self::GasCredit(event) => event.base.event_id.clone(),
            Self::GasRefunded(event) => event.base.event_id.clone(),
            Self::Call(event) => event.base.event_id.clone(),
            Self::MessageApproved(event) => event.base.event_id.clone(),
            Self::MessageExecuted(event) => event.base.event_id.clone(),
            Self::CannotExecuteMessageV2(evens) => evens.base.event_id.clone(),
            Self::SignersRotated(event) => event.base.event_id.clone(),
        }
    }
}

impl Display for Event {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::GasCredit(event) => {
                for line in event.to_string().lines() {
                    writeln!(f, "{line}")?;
                }
            }
            Self::GasRefunded(event) => {
                for line in event.to_string().lines() {
                    writeln!(f, "{line}")?;
                }
            }
            Self::Call(event) => {
                for line in event.to_string().lines() {
                    writeln!(f, "{line}")?;
                }
            }
            Self::MessageApproved(event) => {
                for line in event.to_string().lines() {
                    writeln!(f, "{line}")?;
                }
            }
            Self::MessageExecuted(event) => {
                for line in event.to_string().lines() {
                    writeln!(f, "{line}")?;
                }
            }
            Self::CannotExecuteMessageV2(event) => {
                for line in event.to_string().lines() {
                    writeln!(f, "{line}")?;
                }
            }
            Self::SignersRotated(event) => {
                for line in event.to_string().lines() {
                    writeln!(f, "{line}")?;
                }
            }
        }
        Ok(())
    }
}

/// Represents the request payload for posting events.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct PublishEventsRequest {
    /// list of events to publish
    pub events: Vec<Event>,
}

impl Display for PublishEventsRequest {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        writeln!(f, "PublishEventsRequest:")?;
        writeln!(f, "  events:")?;
        for (index, event) in self.events.iter().enumerate() {
            writeln!(f, "    [{index}]:")?;
            for line in event.to_string().lines() {
                writeln!(f, "      {line}")?;
            }
        }
        Ok(())
    }
}

/// Base struct for publish event result items.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct PublishEventResultItemBase {
    /// index of the event
    pub index: usize,
}

/// Represents an accepted publish event result.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct PublishEventAcceptedResult {
    /// event base
    #[serde(flatten)]
    pub base: PublishEventResultItemBase,
}

/// Represents an error in publishing an event.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct PublishEventErrorResult {
    /// event base
    #[serde(flatten)]
    pub base: PublishEventResultItemBase,
    /// error message
    pub error: String,
    /// weather we can retry publishing the event
    pub retriable: bool,
}

/// Represents the result of processing an event.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(tag = "status", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PublishEventResultItem {
    /// the event was accepted
    Accepted(PublishEventAcceptedResult),
    /// could not accept the event
    Error(PublishEventErrorResult),
}

/// Represents the response from posting events.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct PublishEventsResult {
    /// The result array
    pub results: Vec<PublishEventResultItem>,
}

/// Represents a Verify Task.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct VerifyTask {
    /// the cross chain message
    pub message: GatewayV2Message,
    /// the raw payload of the task
    #[serde(
        deserialize_with = "serde_utils::base64_decode",
        serialize_with = "serde_utils::base64_encode"
    )]
    pub payload: Vec<u8>,
}

/// Represents a Construct Proof Task.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct ConstructProofTask {
    /// the cross chain message
    pub message: GatewayV2Message,
    /// the raw payload of the task
    #[serde(
        deserialize_with = "serde_utils::base64_decode",
        serialize_with = "serde_utils::base64_encode"
    )]
    pub payload: Vec<u8>,
}

/// Represents a Gateway Transaction Task.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct GatewayTransactionTask {
    /// the execute data for the gateway
    #[serde(
        deserialize_with = "serde_utils::base64_decode",
        serialize_with = "serde_utils::base64_encode"
    )]
    #[serde(rename = "executeData")]
    pub execute_data: Vec<u8>,
}

/// Represents an Execute Task.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct ExecuteTask {
    /// the cross-chain message
    pub message: GatewayV2Message,
    #[serde(
        deserialize_with = "serde_utils::base64_decode",
        serialize_with = "serde_utils::base64_encode"
    )]
    /// the raw payload
    pub payload: Vec<u8>,
    /// The gas balance we currently have left
    #[serde(rename = "availableGasBalance")]
    pub available_gas_balance: Token,
}

/// Represents a Refund Task.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct RefundTask {
    /// the cross-chain message
    pub message: GatewayV2Message,
    #[serde(rename = "refundRecipientAddress")]
    /// the address that will receive the balance
    pub refund_recipient_address: Address,
    #[serde(rename = "remainingGasBalance")]
    /// how much balance is left post-refund
    pub remaining_gas_balance: Token,
}

/// Represents a generic Task, which can be any of the specific task types.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(tag = "type", content = "task", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Task {
    /// Construct Proof
    ConstructProof(ConstructProofTask),
    /// Verify task
    Verify(VerifyTask),
    /// Gateway TX task
    GatewayTx(GatewayTransactionTask),
    /// Execute task
    Execute(ExecuteTask),
    /// Refund task
    Refund(RefundTask),
}

/// Represents an individual Task Item.
#[derive(
    Clone, PartialEq, Eq, Serialize, Deserialize, TypedBuilder, BorshSerialize, BorshDeserialize,
)]
pub struct TaskItem {
    /// UUID of current task
    pub id: TaskItemId,
    /// timestamp of task’s creation
    #[borsh(
        serialize_with = "crate::util::serialize_utc",
        deserialize_with = "crate::util::deserialize_utc"
    )]
    pub timestamp: DateTime<Utc>,
    /// the inner task
    #[serde(flatten)]
    pub task: Task,
}

impl QueueMsgId for TaskItem {
    type MessageId = TaskItemId;

    fn id(&self) -> Self::MessageId {
        self.id.clone()
    }
}

#[expect(clippy::min_ident_chars, reason = "comes from trait definition")]
impl core::fmt::Debug for TaskItem {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let task_type = match self.task {
            Task::Verify(_) => "Verify",
            Task::GatewayTx(_) => "GatewayTx",
            Task::Execute(_) => "Execute",
            Task::Refund(_) => "Refund",
            Task::ConstructProof(_) => "ConstructProof",
        };

        f.debug_struct("TaskItem")
            .field("id", &self.id)
            .field("timestamp", &self.timestamp)
            .field("task", &task_type)
            .finish()
    }
}

/// Represents the response from fetching tasks.
#[derive(
    Clone,
    Debug,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    TypedBuilder,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct GetTasksResult {
    /// Array of tasks matching the filters
    pub tasks: Vec<TaskItem>,
}

/// Represents an error response.
#[derive(
    Debug, thiserror::Error, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
#[error("Amplifier API Error: {error}")]
pub struct ErrorResponse {
    /// error message
    pub error: String,
    /// the request id
    #[serde(rename = "requestID")]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_id: Option<RequestId>,
}

mod big_int {
    use serde::{Deserialize, Deserializer, Serialize};

    /// Represents a big integer as a string matching the pattern `^(0|[1-9]\d*)$`.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct BigInt(pub bnum::types::I512);
    impl BigInt {
        /// Creates a new [`BigInt`].
        #[must_use]
        pub const fn new(num: bnum::types::I512) -> Self {
            Self(num)
        }

        /// Helper utility to transform u64 into a `BigInt`
        #[must_use]
        pub fn from_u64(num: u64) -> Self {
            Self(num.into())
        }
    }

    impl Serialize for BigInt {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            let string = self.0.to_string();
            serializer.serialize_str(&string)
        }
    }

    impl<'de> Deserialize<'de> for BigInt {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let string = String::deserialize(deserializer)?;
            let number = bnum::types::I512::parse_str_radix(string.as_str(), 10);
            Ok(Self(number))
        }
    }
}
/// reference types are copied from the following documentation [link](https://bright-ambert-2bd.notion.site/Amplifier-GMP-API-EXTERNAL-911e740b570b4017826c854338b906c8#e8a7398607bd496eb0b8e95e887d6574)
#[cfg(test)]
mod tests {
    use base64::prelude::*;
    use pretty_assertions::assert_eq;
    use serde::de::DeserializeOwned;
    use simd_json::{from_slice, json, to_owned_value, to_string};

    use super::*;

    const BASE64_PAYLOAD: &str = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAASaGVsbG8gdGVzdC1zZXBvbGlhAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==";
    const BASE64_PAYLOAD_HASH: &str = "Y2YO3UuCRRackxbPYX9dWmNTYcnAMOommp9g4ydb3i4=";

    fn payload_bytes() -> Vec<u8> {
        BASE64_STANDARD.decode(BASE64_PAYLOAD).unwrap()
    }
    fn payload_hash_bytes() -> Vec<u8> {
        BASE64_STANDARD.decode(BASE64_PAYLOAD_HASH).unwrap()
    }

    fn test_serialization<T>(type_in_rust: &T, mut reference_json: Vec<u8>)
    where
        T: Serialize + DeserializeOwned + PartialEq + core::fmt::Debug,
    {
        // Action
        let mut type_in_rust_as_json = to_string(&type_in_rust).unwrap().into_bytes();

        // Assert
        assert_eq!(
            to_owned_value(type_in_rust_as_json.as_mut_slice()).unwrap(),
            to_owned_value(reference_json.as_mut_slice()).unwrap(),
            "During serialization process, the Rust type diverged from the reference"
        );

        let deserialized: T = from_slice(type_in_rust_as_json.as_mut_slice()).unwrap();

        assert_eq!(
            type_in_rust, &deserialized,
            "Deserialization caused the type to malform"
        );
    }
    #[test]
    fn test_gas_credit_event_serialization1() {
        // Setup
        let reference_json = simd_json::to_string(&json!({
            "eventID": "0x1fbe533b3bed6e6be3a74734042b3ae0836cabfdc2b646358080db75e9fe9ba8-1",
            "meta": {
                "txID": "0x1fbe533b3bed6e6be3a74734042b3ae0836cabfdc2b646358080db75e9fe9ba8",
                "timestamp": "2024-09-11T13:32:48Z",
                "fromAddress": "0xEA12282BaC49497793622d67e2CD43bf1065a819",
                "finalized": true
            },
            "messageID": "0x1fbe533b3bed6e6be3a74734042b3ae0836cabfdc2b646358080db75e9fe9ba8-2",
            "refundAddress": "0xEA12282BaC49497793622d67e2CD43bf1065a819",
            "payment": {
                "amount": "410727029715539"
            }
        }))
        .unwrap()
        .into_bytes();
        let type_in_rust = GasCreditEvent {
            base: EventBase {
                event_id: EventId::new(
                    "0x1fbe533b3bed6e6be3a74734042b3ae0836cabfdc2b646358080db75e9fe9ba8",
                    1,
                ),
                meta: Some(EventMetadata {
                    tx_id: Some(TxId(
                        "0x1fbe533b3bed6e6be3a74734042b3ae0836cabfdc2b646358080db75e9fe9ba8"
                            .to_owned(),
                    )),
                    timestamp: DateTime::parse_from_rfc3339("2024-09-11T13:32:48Z")
                        .map(|x| x.to_utc())
                        .ok(),
                    from_address: Some("0xEA12282BaC49497793622d67e2CD43bf1065a819".to_owned()),
                    finalized: Some(true),
                    extra: (),
                }),
            },
            message_id: MessageId::new(
                "0x1fbe533b3bed6e6be3a74734042b3ae0836cabfdc2b646358080db75e9fe9ba8",
                2,
            ),
            refund_address: "0xEA12282BaC49497793622d67e2CD43bf1065a819".to_owned(),
            payment: Token {
                token_id: None,
                amount: BigInt::from_u64(410_727_029_715_539),
            },
        };

        // Action
        test_serialization(&type_in_rust, reference_json);
    }

    #[test]
    fn test_gas_credit_event_serialization() {
        // Setup
        let reference_json = to_string(&json!({
            "eventID": "0x1fbe533b3bed6e6be3a74734042b3ae0836cabfdc2b646358080db75e9fe9ba8-1",
            "meta": {
                "txID": "0x1fbe533b3bed6e6be3a74734042b3ae0836cabfdc2b646358080db75e9fe9ba8",
                "timestamp": "2024-09-11T13:32:48Z",
                "fromAddress": "0xEA12282BaC49497793622d67e2CD43bf1065a819",
                "finalized": true
            },
            "messageID": "0x1fbe533b3bed6e6be3a74734042b3ae0836cabfdc2b646358080db75e9fe9ba8-2",
            "refundAddress": "0xEA12282BaC49497793622d67e2CD43bf1065a819",
            "payment": {
                "amount": "410727029715539"
            }
        }))
        .unwrap()
        .into_bytes();

        let type_in_rust = GasCreditEvent {
            base: EventBase {
                event_id: MessageId::new(
                    "0x1fbe533b3bed6e6be3a74734042b3ae0836cabfdc2b646358080db75e9fe9ba8",
                    1,
                ),
                meta: Some(EventMetadata {
                    tx_id: Some(TxId(
                        "0x1fbe533b3bed6e6be3a74734042b3ae0836cabfdc2b646358080db75e9fe9ba8"
                            .to_owned(),
                    )),
                    timestamp: DateTime::parse_from_rfc3339("2024-09-11T13:32:48Z")
                        .map(|x| x.to_utc())
                        .ok(),
                    from_address: Some("0xEA12282BaC49497793622d67e2CD43bf1065a819".to_owned()),
                    finalized: Some(true),
                    extra: (),
                }),
            },
            message_id: MessageId::new(
                "0x1fbe533b3bed6e6be3a74734042b3ae0836cabfdc2b646358080db75e9fe9ba8",
                2,
            ),
            refund_address: "0xEA12282BaC49497793622d67e2CD43bf1065a819".to_owned(),
            payment: Token {
                token_id: None,
                amount: BigInt::from_u64(410_727_029_715_539),
            },
        };

        test_serialization(&type_in_rust, reference_json);
    }

    #[test]
    fn test_call_event_serialization() {
        // Setup
        let reference_json = to_string(&json!({
            "eventID": "0xe432150cce91c13a887f7D836923d5597adD8E31-2",
            "message": {
                "messageID": "0xe432150cce91c13a887f7D836923d5597adD8E31-2",
                "sourceChain": "ethereum",
                "sourceAddress": "0xEA12282BaC49497793622d67e2CD43bf1065a819",
                "destinationAddress": "0xf4ff91f79E35E7dF460A6B259fD971Ec85E933CF",
                "payloadHash": BASE64_PAYLOAD_HASH
            },
            "destinationChain": "Avalanche",
            "payload": BASE64_PAYLOAD
        }))
        .unwrap()
        .into_bytes();

        let type_in_rust = CallEvent {
            base: EventBase {
                event_id: EventId::new("0xe432150cce91c13a887f7D836923d5597adD8E31", 2),
                meta: None,
            },
            message: GatewayV2Message {
                message_id: MessageId::new("0xe432150cce91c13a887f7D836923d5597adD8E31", 2),
                source_chain: "ethereum".to_owned(),
                source_address: "0xEA12282BaC49497793622d67e2CD43bf1065a819".to_owned(),
                destination_address: "0xf4ff91f79E35E7dF460A6B259fD971Ec85E933CF".to_owned(),
                payload_hash: payload_hash_bytes(),
            },
            destination_chain: "Avalanche".to_owned(),
            payload: payload_bytes(),
        };

        test_serialization(&type_in_rust, reference_json);
    }

    #[test]
    fn test_event_enum_serialization() {
        // Setup
        let reference_json = to_string(&json!({
            "type": "CALL",
            "eventID": "0xe432150cce91c13a887f7D836923d5597adD8E31-2",
            "message": {
                "messageID": "0xe432150cce91c13a887f7D836923d5597adD8E31-2",
                "sourceChain": "ethereum",
                "sourceAddress": "0xEA12282BaC49497793622d67e2CD43bf1065a819",
                "destinationAddress": "0xf4ff91f79E35E7dF460A6B259fD971Ec85E933CF",
                "payloadHash": BASE64_PAYLOAD_HASH
            },
            "destinationChain": "Avalanche",
            "payload": BASE64_PAYLOAD
        }))
        .unwrap()
        .into_bytes();

        let type_in_rust = Event::Call(CallEvent {
            base: EventBase {
                event_id: EventId::new("0xe432150cce91c13a887f7D836923d5597adD8E31", 2),
                meta: None,
            },
            message: GatewayV2Message {
                message_id: MessageId::new("0xe432150cce91c13a887f7D836923d5597adD8E31", 2),
                source_chain: "ethereum".to_owned(),
                source_address: "0xEA12282BaC49497793622d67e2CD43bf1065a819".to_owned(),
                destination_address: "0xf4ff91f79E35E7dF460A6B259fD971Ec85E933CF".to_owned(),
                payload_hash: payload_hash_bytes(),
            },
            destination_chain: "Avalanche".to_owned(),
            payload: payload_bytes(),
        });

        test_serialization(&type_in_rust, reference_json);
    }

    #[test]
    fn test_tasks_response_deserialization() {
        // Setup
        let execute_data = vec![111];
        let execute_data_encoded = BASE64_STANDARD.encode(&execute_data);

        let reference_json = to_string(&json!({
              "tasks": [{
                "id": "550e8400-e29b-41d4-a716-446655440000",
                "timestamp": "2023-04-01T12:00:00Z",
                "type": "GATEWAY_TX",
                "task": {
                  "executeData": execute_data_encoded
                }
              }]
        }))
        .unwrap()
        .into_bytes();

        let type_in_rust = GetTasksResult {
            tasks: vec![TaskItem {
                id: TaskItemId("550e8400-e29b-41d4-a716-446655440000".parse().unwrap()),
                timestamp: DateTime::parse_from_rfc3339("2023-04-01T12:00:00Z")
                    .map(|x| x.to_utc())
                    .unwrap(),
                task: Task::GatewayTx(GatewayTransactionTask { execute_data }),
            }],
        };
        test_serialization(&type_in_rust, reference_json);
    }

    #[test]
    fn test_publish_events_request_serialization() {
        // Setup
        let reference_json = to_string(&json!({
            "events": [
                {
                    "type": "CALL",
                    "eventID": "0x9b447614be654eeea0c5de0319b3f2c243ab45bebd914a1f7319f4bb599d8968-1",
                    "meta": {
                        "txID": "0x9b447614be654eeea0c5de0319b3f2c243ab45bebd914a1f7319f4bb599d8968",
                        "timestamp": "2024-09-11T13:32:48Z",
                        "fromAddress": "0xba76c6980428A0b10CFC5d8ccb61949677A61233",
                        "finalized": true
                    },
                    "message": {
                        "messageID": "0x9b447614be654eeea0c5de0319b3f2c243ab45bebd914a1f7319f4bb599d8968-1",
                        "sourceChain": "test-sepolia",
                        "sourceAddress": "0x9e3e785dD9EA3826C9cBaFb1114868bc0e79539a",
                        "destinationAddress": "0xE8E348fA7b311d6E308b1A162C3ec0172B37D1C1",
                        "payloadHash": BASE64_PAYLOAD_HASH
                    },
                    "destinationChain": "test-avalanche",
                    "payload": BASE64_PAYLOAD
                }
            ]
        }))
        .unwrap()
        .into_bytes();

        let type_in_rust =
            PublishEventsRequest {
                events: vec![Event::Call(CallEvent {
                base: EventBase {
                    event_id: EventId::new(
                        "0x9b447614be654eeea0c5de0319b3f2c243ab45bebd914a1f7319f4bb599d8968",
                        1,
                    ),
                    meta: Some(EventMetadata {
                        tx_id: Some(TxId(
                            "0x9b447614be654eeea0c5de0319b3f2c243ab45bebd914a1f7319f4bb599d8968"
                                .to_owned(),
                        )),
                        timestamp: DateTime::parse_from_rfc3339("2024-09-11T13:32:48Z")
                            .map(|x| x.to_utc())
                            .ok(),
                        from_address: Some("0xba76c6980428A0b10CFC5d8ccb61949677A61233".to_owned()),
                        finalized: Some(true),
                        extra: CallEventMetadata { parent_message_id: None },
                    }),
                },
                message: GatewayV2Message {
                    message_id: MessageId::new(
                        "0x9b447614be654eeea0c5de0319b3f2c243ab45bebd914a1f7319f4bb599d8968",
                        1,
                    ),
                    source_chain: "test-sepolia".to_owned(),
                    source_address: "0x9e3e785dD9EA3826C9cBaFb1114868bc0e79539a".to_owned(),
                    destination_address: "0xE8E348fA7b311d6E308b1A162C3ec0172B37D1C1".to_owned(),
                    payload_hash: payload_hash_bytes(),
                },
                destination_chain: "test-avalanche".to_owned(),
                payload: payload_bytes(),
            })],
            };

        test_serialization(&type_in_rust, reference_json);
    }

    #[test]
    fn test_payload_base64_serialization() {
        // Setup
        let type_in_rust = CallEvent {
            base: EventBase {
                event_id: TxEvent("event123".to_owned()),
                meta: None,
            },
            message: GatewayV2Message {
                message_id: TxEvent("message123".to_owned()),
                source_chain: "chainA".to_owned(),
                source_address: "0xSourceAddress".to_owned(),
                destination_address: "0xDestinationAddress".to_owned(),
                payload_hash: payload_hash_bytes(),
            },
            destination_chain: "chainB".to_owned(),
            payload: payload_bytes(),
        };

        let reference_json = to_string(&json!({
            "eventID": "event123",
            "message": {
                "messageID": "message123",
                "sourceChain": "chainA",
                "sourceAddress": "0xSourceAddress",
                "destinationAddress": "0xDestinationAddress",
                "payloadHash": BASE64_PAYLOAD_HASH
            },
            "destinationChain": "chainB",
            "payload": BASE64_PAYLOAD
        }))
        .unwrap()
        .into_bytes();

        test_serialization(&type_in_rust, reference_json);
    }

    #[test]
    fn test_cannot_execute_message_v2_serialization() {
        // Setup
        let reference_json = to_string(&json!({
            "type":"CANNOT_EXECUTE_MESSAGE/V2",
            "eventID":"event123",
            "meta":{
               "taskItemID":"0193c0ec-20ac-7d8c-8e71-a0b94d1c7a5d"
            },
            "messageID":"0x413b6134d65fe844b0d9e541561a5e825240fb4d951e34e5f42806ef9d45c560-0",
            "sourceChain":"avalanche-fuji",
            "reason":"ERROR",
            "details":"some details here"
        }))
        .unwrap()
        .into_bytes();

        let type_in_rust = Event::CannotExecuteMessageV2(CannotExecuteMessageEventV2 {
            source_chain: "avalanche-fuji".to_owned(),
            message_id: TxEvent(
                "0x413b6134d65fe844b0d9e541561a5e825240fb4d951e34e5f42806ef9d45c560-0".to_owned(),
            ),
            base: EventBase {
                event_id: TxEvent("event123".to_owned()),
                meta: Some(EventMetadata::<CannotExecuteMessageEventV2Metadata> {
                    extra: CannotExecuteMessageEventV2Metadata {
                        task_item_id: TaskItemId(
                            "0193c0ec-20ac-7d8c-8e71-a0b94d1c7a5d".parse().unwrap(),
                        ),
                        from_address: None,
                        timestamp: None,
                    },
                    tx_id: None,
                    timestamp: None,
                    from_address: None,
                    finalized: None,
                }),
            },
            reason: CannotExecuteMessageReason::Error,
            details: "some details here".to_owned(),
        });

        test_serialization(&type_in_rust, reference_json);
    }

    #[test]
    fn can_deserialize_error() {
        let reference_json = br#"{"error":"no matching operation was found"}"#;
        let type_in_rust = ErrorResponse {
            error: "no matching operation was found".to_owned(),
            request_id: None,
        };

        test_serialization(&type_in_rust, reference_json.to_vec());
    }

    #[test]
    fn test_deserialize_tasks_response() {
        // Given JSON
        let mut json_data = r#"
        {
            "tasks": [
                {
                    "id": "01924c97-a26b-7eff-8289-c3bdf2b37446",
                    "task": {
                        "executeData": "YXZhbGFuY2hlLWZ1amkweDUxZWM2MmI0YWI0YzY1OTM4YTNmZTNlMjlhN2Y4OTMwYzkyODk3MWI1ZTc5MTQxODA4ZjI3OTZlYjgxYzU4NzItMDB4NDM2NmEwNDFiQTQyMzdGOWI3NTUzQjhhNzNlOEFGMWYyZWUxRjRkMXNvbGFuYS1kZXZuZXRtZW1RdUtNR0JvdW5od1A1eXc5cW9tWU5VOTdFcWN4OWM0WHdEVW82dUdWzjepvJoJHFQzw1uyXXd48x3os6pzUWvq8CLZoHNYpLgOAAAALP///0QAAAAy////KgAAAG7///8NAAAAkP///ysAAACV////AAAAAAAAAAABAAAAAAOkQJ+JUVaKfZGWXgR9L4KOCebkpdpCe1FfVVxF9+CBMwEAtcL4qBLWMHodl+/x6UzKZ+1v6InbUUK82UTyJPGkN2Vev/IRM1aZxgGVS97+qW8mfehYwHvk69Ei0masgbXJYhwBAAAAAAAAAAAAAAAAAAAAAAAAAAAAADD///8BAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAdF8wAAAAAAB0XzAAAAAAAAEAAAAg////AQAAAAAAAAA="
                    },
                    "timestamp": "2024-10-02T09:37:39.608929Z",
                    "type": "GATEWAY_TX"
                },
                {
                    "id": "0192d87b-faab-7dd7-9750-f6261541cf2b",
                    "task": {
                        "executeData": "YXZhbGFuY2hlLWZ1amkweDFiM2RkNmI2OTYyZmE3OWQ1NzFmNjAxMjhiMGY0OTIyNzQ4ODM1NDNjNGQ0MDg5YTdjMzZmYjQ3NGFmNDVkZWItMDB4RTJjZEI0MDQwMDM0ZTA1Yjc4RDUzYUMzMjIwNWEzNDdhMjEzYzkwNXNvbGFuYS1kZXZuZXRtZW1RdUtNR0JvdW5od1A1eXc5cW9tWU5VOTdFcWN4OWM0WHdEVW82dUdWbom8u2+OKANkGqUpecD+cRHd4C+YjMPyDduiSvwOm4QOAAAALP///0QAAAAy////KgAAAG7///8NAAAAkP///ysAAACV////AAAAAAAAAAABAAAAAAOkQJ+JUVaKfZGWXgR9L4KOCebkpdpCe1FfVVxF9+CBMwEApmIkQcyrccWA6IGBMyDrCbWfDlSpCkBEzVzAz3FFd68bCnvViSwHWdx71h/6JYKOii2fFP4haZrn2c3+Wkip6xwBAAAAAAAAAAAAAAAAAAAAAAAAAAAAADD///8BAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAdF8wAAAAAAB0XzAAAAAAAAEAAAAg////AQAAAAAAAAA="
                    },
                    "timestamp": "2024-10-29T13:34:16.754126Z",
                    "type": "GATEWAY_TX"
                },
                {
                    "id": "0192d881-9433-7831-8c67-98eaa7727676",
                    "task": {
                        "availableGasBalance": {
                            "amount": "-864042"
                        },
                        "message": {
                            "destinationAddress": "memQuKMGBounhwP5yw9qomYNU97Eqcx9c4XwDUo6uGV",
                            "messageID": "0x1b3dd6b6962fa79d571f60128b0f492274883543c4d4089a7c36fb474af45deb-0",
                            "payloadHash": "bom8u2+OKANkGqUpecD+cRHd4C+YjMPyDduiSvwOm4Q=",
                            "sourceAddress": "0xE2cdB4040034e05b78D53aC32205a347a213c905",
                            "sourceChain": "avalanche-fuji"
                        },
                        "payload": "AQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAIAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAACWhlbGxv8J+QqgAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAHo0Z7lybs+RcfP+DxXZV1GDSdTSXtHjzHyWWXizQKXPQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAE="
                    },
                    "timestamp": "2024-10-29T13:40:23.732532Z",
                    "type": "EXECUTE"
                },
                {
                    "id": "0192d882-0310-780c-bda4-47770cce7248",
                    "task": {
                        "executeData": "YXZhbGFuY2hlLWZ1amkweGVkZTFkYWY1ZmEyZjJkZTAwNGQ0NzljMjRhMjJmNDI5YmFiOGZhOGQwMTgxOWNhNzZmN2JlN2VmYjNmNGUyZjUtMDB4RTJjZEI0MDQwMDM0ZTA1Yjc4RDUzYUMzMjIwNWEzNDdhMjEzYzkwNXNvbGFuYS1kZXZuZXRtZW1RdUtNR0JvdW5od1A1eXc5cW9tWU5VOTdFcWN4OWM0WHdEVW82dUdWbom8u2+OKANkGqUpecD+cRHd4C+YjMPyDduiSvwOm4QOAAAALP///0QAAAAy////KgAAAG7///8NAAAAkP///ysAAACV////AAAAAAAAAAABAAAAAAOkQJ+JUVaKfZGWXgR9L4KOCebkpdpCe1FfVVxF9+CBMwEAzQXXxwk7hn3x8p6/PzqdKGric/f1xyVOxChGshfK1G08/QWRtLexC6M5+aAYadUXaJkGYmYP0F0bPhYDJ0be4xwBAAAAAAAAAAAAAAAAAAAAAAAAAAAAADD///8BAAAAAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABAAAAAAAAAAAAAAAAAAAAdF8wAAAAAAB0XzAAAAAAAAEAAAAg////AQAAAAAAAAA="
                    },
                    "timestamp": "2024-10-29T13:40:52.1338Z",
                    "type": "GATEWAY_TX"
                }
            ]
        }
        "#.to_owned()
        .into_bytes();

        let _deserialized: GetTasksResult = from_slice(json_data.as_mut_slice()).unwrap();
    }
}
