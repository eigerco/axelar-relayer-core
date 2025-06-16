//! Nats constants

/// Amplifier Events Stream Name
pub const EVENTS_STREAM: &str = "AMPLIFIER_EVENTS";
/// Amplifier Events Publish Subjesct Name
pub const EVENTS_PUBLISH_SUBJECT: &str = "amplifier.event.new";
/// Amplifier ingesters consume group
pub const EVENTS_CONSUME_GROUP: &str = "amplifier-ingesters";

/// Amplifier Tasks Stream Name
pub const TASKS_STREAM: &str = "AMPLIFIER_TASKS";
/// Amplifier Tasks Publish Subject Name
pub const TASKS_PUBLISH_SUBJECT: &str = "amplifier.tasks.new";
