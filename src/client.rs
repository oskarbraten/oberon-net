use crossbeam::channel;

use crate::{Event, Message};

#[derive(Debug, Clone)]
pub struct Client {
    sender: channel::Sender<Message>,
    receiver: channel::Receiver<Event>,
}
