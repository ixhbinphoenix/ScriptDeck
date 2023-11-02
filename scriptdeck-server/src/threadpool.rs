use std::sync::Arc;
use std::thread::{JoinHandle, self};
use actix::{Addr, Message};
use crossbeam_channel::{Receiver, Sender, unbounded};
use log::{info, error, debug};
use rhai::{AST, Engine, Dynamic, ImmutableString};

use crate::ws::Global;

enum Pool2Runner {
    ByeBye,
    RunRhai(AST, u32)
}

#[derive(Debug, Message)]
#[rtype(result = "()")]
pub enum R2G {
    Reply(String, u32),
    Broadcast(String),
    InternalError(u32)
}

pub struct RunnerPool {
    max_runners: usize,
    runners: Vec<Runner>,
    started: bool,
    sender: Option<Sender<Pool2Runner>>
}

impl RunnerPool {
    pub fn new(max_runners: usize) -> RunnerPool {
        if max_runners == 0 {
            panic!("max_runners must be greater than zero!");
        }

        RunnerPool { max_runners, started: false, runners: Vec::new(), sender: None }
    }

    pub fn start(&mut self, global: Addr<Global>) {
        assert!(!self.started);
        let (p2rtx, p2rrx) = unbounded();
        self.sender = Some(p2rtx);

        info!("Starting {} runners", self.max_runners);
        let receiver = Arc::new(p2rrx);
        for i in 0..self.max_runners {
            self.runners.push(Runner::new(i, global.clone(), Arc::clone(&receiver)));
        }
        self.started = true;
    }

    pub fn execute(&self, script: AST, local_id: u32) {
        assert!(self.started);
        let _ = self.sender.clone().unwrap().send(Pool2Runner::RunRhai(script, local_id));
    }
}

impl Drop for RunnerPool {
    fn drop(&mut self) {
        for _ in 0..self.max_runners {
            if let Some(sender) = &self.sender {
                let _ = sender.send(Pool2Runner::ByeBye);
            }
        }
        for w in self.runners.iter_mut() {
            if let Some(t) = w.handle.take() {
                t.join().unwrap();
            }
        }
    }
}

struct Runner {
    _id: usize,
    handle: Option<JoinHandle<()>>,
}

impl Runner {
    fn new(id: usize, global: Addr<Global>, receiver: Arc<Receiver<Pool2Runner>>) -> Self {
        let t = thread::spawn(move || {
            let mut engine = Engine::new();

            let glob = global.clone();
            engine.register_fn("broadcast", move |msg: ImmutableString| {
                glob.do_send(R2G::Broadcast(msg.to_string()))
            });

            let glob = global.clone();
            // TODOO: Look for a way to remove the caller_id argument from this and all other potential functions
            engine.register_fn("reply", move |msg: ImmutableString, caller: i64| {
                let id: u32 = match caller.try_into() {
                    Ok(a) => a,
                    Err(_) => {
                        error!("Script called reply with invalid caller id");
                        return
                    },
                };

                glob.do_send(R2G::Reply(msg.to_string(), id))
            });

            debug!("runner {id} started");
            let rx = receiver;
            loop {
                let message = rx.recv().unwrap();
                match message {
                    Pool2Runner::ByeBye => {
                        debug!("runner {id} stopped");
                        return;
                    }
                    Pool2Runner::RunRhai(ast, id) => {
                        match engine.eval_ast::<Dynamic>(&ast) {
                            Ok(a) => {
                                if a.is_string() {
                                    global.do_send(R2G::Reply(a.into_string().unwrap(), id));
                                }
                            },
                            Err(e) => {
                                error!("Error running script: {e}");
                                global.do_send(R2G::InternalError(id));
                            },
                        };
                    }
                }
            }
        });

        Runner { _id: id, handle: Some(t) }
    }
}
