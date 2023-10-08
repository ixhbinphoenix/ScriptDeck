use std::{collections::HashMap, sync::{Arc, RwLock}, path::PathBuf, fs};
use actix::{Actor, Addr, AsyncContext, Context, Recipient, StreamHandler, Handler, Message};
use actix_web::web;
use actix_web_actors::ws;
use log::{debug, info, error, warn};
use rhai::{AST, Dynamic, Engine, ImmutableString};

use crate::proto::{C2S, C2SLocal, C2SGlobal, S2C, L2G};
use crate::new_shared;

pub type Shared<T> = Arc<RwLock<T>>;

/// Local Connection Handler
///
/// Receives incoming connections, parses messages and either:
/// 1. Processes them locally ([proto::C2SLocal])
/// 2. Passes them to the [Global] handler ([proto::C2SGlobal])
pub struct Local(pub web::Data<Addr<Global>>, pub usize);

impl Actor for Local {
    type Context = ws::WebsocketContext<Self>;

    /// Automatically register Local handler to the Global handler
    fn started(&mut self, ctx: &mut Self::Context) {
        let addr = ctx.address();
        // TODO: Check if the message is received, cause otherwise we got a problem
        self.0.do_send(Connect(addr, self.1))
    }

    /// Automatically de-register Local handler from Global handler after the connection is closed
    fn stopped(&mut self, _: &mut Self::Context) {
        self.0.do_send(Disconnect(self.1))
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for Local {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        debug!("[Local ({})] Received message", self.1);
        use ws::Message as M;
        match msg {
            Ok(M::Ping(msg)) => ctx.pong(&msg),
            Ok(M::Text(text)) => {
                match serde_json::from_str::<C2S>(&text) {
                    Ok(msg) => {
                        match msg {
                            C2S::Global(c2sg) => {
                                debug!("[Local ({})] C2SGlobal, passing to global", self.1);
                                let msg = L2G {
                                    msg: c2sg,
                                    local_id: self.1
                                };
                                self.0.do_send(msg);
                            },
                            C2S::Local(msg) => {
                                match msg {
                                    C2SLocal::GetID => {
                                        info!("[Local ({})] GetID", self.1);
                                        ctx.text(serde_json::to_string(&S2C::ClientID { id: self.1 }).unwrap());
                                    }
                                }
                            }
                        }
                    },
                    Err(_) => {
                        warn!("[Local ({})] Received invalid message", self.1);
                        // Won't panic, we know that this is valid
                        ctx.text(serde_json::to_string(&S2C::BadRequest).unwrap());
                    },
                }
            },
            _ => {
                warn!("[Local ({})] Received invalid message type (not text or ping)", self.1);
                // Won't panic, we know that this is valid
                ctx.text(serde_json::to_string(&S2C::BadRequest).unwrap());
            },
        }
    }
}

impl Handler<S2C> for Local {
    type Result = ();
    /// Just passes S2C Messages onto the client
    fn handle(&mut self, msg: S2C, ctx: &mut Self::Context) -> Self::Result {
        ctx.text(match serde_json::to_string(&msg) {
            Ok(a) => a,
            Err(e) => {
                error!("Error serializing S2C message, {e}");
                debug!("S2C Message: {:?}", &msg);
                return
            }
        });
    }
}

/// Global State Handler
///
/// Receives messages from the local handlers for operations that might or definelty require global
/// state, e.g. Rhai Script execution
pub struct Global {
    pub sessions: Shared<HashMap<usize, Recipient<S2C>>>,
    pub engine: Engine,
    pub ast_cache: HashMap<String, AST>
}

impl Actor for Global {
    type Context = Context<Self>;
}

impl Default for Global {
    fn default() -> Self {
        let mut glob = Self {
            sessions: new_shared!(HashMap::new()),
            engine: Engine::new(),
            ast_cache: HashMap::new()
        };

        let sessions = glob.sessions.clone();
        glob.engine.register_fn("broadcast", move |msg: ImmutableString| {
            // Surely this will never panic
            let sessions = sessions.read().unwrap();
            for ele in sessions.iter() {
                ele.1.do_send(S2C::Broadcast { msg: msg.to_string() })
            }
        });

        let sessions = glob.sessions.clone();
        // TODO: Look for a way to just grab the caller id from the running script? Maybe?
        glob.engine.register_fn("reply", move |msg: ImmutableString, caller: i64| {
            let sessions = sessions.read().unwrap();

            let id: usize = match caller.try_into() {
                Ok(a) => a,
                Err(_) => {
                    error!("Script called reply with negative caller id");
                    return
                },
            };

            match sessions.get(&id) {
                Some(a) => {
                    a.do_send(S2C::Reply { msg: msg.to_string() });
                },
                None => {
                    warn!("Script tried to reply to non-existant caller {}. The Client has probably disconnected.", caller);
                },
            }
        });

        let script_dir = fs::read_dir(Self::RHAI_SCRIPT_PATH);
        match script_dir {
            Ok(dir) => {
                for ele in dir {
                    let name = match ele {
                        Ok(a) => a.file_name(),
                        Err(e) => {warn!("Error reading dir node: {}, Skipping", e);continue;},
                    };
                    if let Err(e) = glob.load_script(name.to_str().unwrap()) {
                        error!("Error loading script: {e}");
                    }
                }
            },
            Err(e) => {
                error!("Error reading script dir, {e}");
            },
        }

        glob
    }
}

impl Global {
    const RHAI_SCRIPT_PATH: &'static str = "src/rhai/";

    /// Load AST and add it to the ast cache
    /// Do not use this function to directly execute the resulting AST as it does not add the Local
    /// id to the header
    fn load_script(&mut self, name: &str) -> anyhow::Result<AST> {
        let path: PathBuf = (Self::RHAI_SCRIPT_PATH.to_string() + name).into();

        let ast = self.engine.compile_file(path)?;

        self.ast_cache.insert(name.to_string(), ast.clone());

        info!("Loaded script {name} into AST cache");

        Ok(ast)
    }

    const LOCAL_AST_TEMPLATE: &'static str = "let caller_id = $LOCAL_ID;";

    /// Loads AST and adds required headers to it, like the caller_id header
    fn get_script(&mut self, name: &str, local_id: usize) -> anyhow::Result<AST> {
        let original_ast = match self.ast_cache.get(name) {
            Some(a) => {
                a.clone()
            },
            None => {
                self.load_script(name)?
            },
        };

        let local_header = Self::LOCAL_AST_TEMPLATE.replace("$LOCAL_ID", format!("{}", local_id).as_str());
        let header_ast = self.engine.compile(local_header)?;

        Ok(header_ast.merge(&original_ast))
    }

    fn do_send_to_client(&self, msg: S2C, client_id: usize) {
        // If this RwLock ever gets poisoned we're FUCKED
        let sessions = self.sessions.read().unwrap();
        match sessions.get(&client_id) {
            Some(c) => {
                c.do_send(msg);
            },
            None => {
                warn!("Tried to send message {:?} to invalid client {client_id}", msg);
            },
        }
    }
}

impl Handler<L2G> for Global {
    type Result = ();
    fn handle(&mut self, msg: L2G, _: &mut Self::Context) -> Self::Result {
        let id = msg.local_id;
        let msg = msg.msg;

        #[allow(unused)]
        use C2SGlobal as MSG;
        match msg {
            MSG::RunScript { script } => {
                let script = match self.get_script(&script, id) {
                    Ok(a) => a,
                    Err(e) => {
                        warn!("Error loading script {script}: {e}");
                        // TODO: Return actually useful errors to the client
                        self.do_send_to_client(S2C::InternalError, id);
                        return
                    },
                };

                match self.engine.eval_ast::<Dynamic>(&script) {
                    Ok(a) => {
                        if a.is_string() {
                            self.do_send_to_client(S2C::Reply { msg: a.into_string().unwrap() }, id);
                        }
                    },
                    Err(e) => {
                        error!("Error running script: {e}");
                        self.do_send_to_client(S2C::InternalError, id);
                    },
                };
            }
        }
    }
}

#[derive(Message)]
#[rtype(result = "()")]
/// Registers a local handler to the global hander with It's address and a randomly generated id
pub struct Connect(Addr<Local>, usize);

impl Handler<Connect> for Global {
    type Result = ();
    fn handle(&mut self, msg: Connect, _: &mut Self::Context) -> Self::Result {
        // Surely this will never panic
        let mut sessions = self.sessions.write().unwrap();
        sessions.insert(msg.1, msg.0.into());
    }
}

#[derive(Message)]
#[rtype(result = "()")]
/// De-registers a local handler from the global handler using the id of the local handler
pub struct Disconnect(usize);

impl Handler<Disconnect> for Global {
    type Result = ();
    fn handle(&mut self, msg: Disconnect, _: &mut Self::Context) -> Self::Result {
        let mut sessions = self.sessions.write().unwrap();
        sessions.remove(&msg.0);
    }
}
