use actix::Message;
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
// If i determine that the global/local split is useless, this should just be C2SGlobal
/// Messages that go Client->Server
pub enum C2S {
    Global(C2SGlobal),
    Local(C2SLocal)
}

#[derive(Debug, Message)]
#[rtype("()")]
/// Wrapper for C2SGlobal providing the id of the local handler
pub struct L2G {
    pub msg: C2SGlobal,
    pub local_id: usize
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
/// Message intended to go to the global handler, needs to be wrapped in [L2G] first
///
/// Passing to global needs to be done when global state, like the rhai engine, is required to
/// process the message
pub enum C2SGlobal {
    /// Runs a rhai script from the src/rhai directory
    RunScript { script: String }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type")]
// Not sure if this is needed, copying from test project
pub enum C2SLocal {
    /// Gets the ID of the Local handler that the client is connected to, and therefore the ID of
    /// the client
    GetID
}

// This is a Message, bc it gets passed from Global -> Local
#[derive(Debug, Serialize, Deserialize, Clone, Message)]
#[serde(tag = "type")]
#[rtype(result = "()")]
// TODO: Add some useful error types
/// Messages that go Server->Client
pub enum S2C {
    /// The client was an utter and complete moron and should not be trusted (hyperbole)
    BadRequest,
    // Is it noticable that I really like backend and hate frontend?
    /// We've made a small mistake, not noteworthy (understatement)
    InternalError,
    /// Message directed at all clients
    Broadcast { msg: String },
    /// Message explicitly directed at the client
    Reply { msg: String },
    /// Returns the ID of the local handler the client is connected to, and therefore the ID of the
    /// client
    ClientID { id: usize }
}
