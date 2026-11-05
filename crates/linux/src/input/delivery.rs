use agent_desktop_core::AdapterError;

use super::delivery_channel::DeliveryChannel;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Delivery {
    pub(crate) delivered_events: u32,
    pub(crate) channel: DeliveryChannel,
}

pub(crate) type Gate<'a> = &'a mut dyn FnMut() -> Result<(), AdapterError>;
