pub mod proto;
pub mod threadpool;
pub mod utils;
pub mod ws;

use std::{sync::{Arc, RwLock}, path::PathBuf};

use actix::{Actor, Addr};
use actix_web::{HttpServer, App, HttpRequest, web, HttpResponse, Error};
use actix_web_actors::ws::start as ws_start;
use clap::Parser;
use dotenvy::dotenv;
use rand::Rng;
use threadpool::RunnerPool;

use crate::ws::{Global, Local};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Amount of Rhai Worker Threads. Defaults to logical cores
    #[arg(long, default_value_t = std::thread::available_parallelism().unwrap().into())]
    rhai_threads: usize,

    /// Specifies the directory for rhai scripts
    #[arg(long, default_value = "src/rhai/")]
    rhai_dir: PathBuf
}

async fn index(req: HttpRequest, stream: web::Payload, global: web::Data<Addr<Global>>) -> Result<HttpResponse, Error> {
    // Collision chance: 1/4294967296
    let id = rand::thread_rng().gen::<u32>();
    ws_start(Local(global, id), &req, stream)
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    let args = Cli::parse();
    dotenv().ok();
    if cfg!(debug_assertions) {
        env_logger::init_from_env(env_logger::Env::new().default_filter_or("debug"));
    } else {
        env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));
    }

    let pool = new_shared!(RunnerPool::new(args.rhai_threads));
    let global = Global::new(Arc::clone(&pool), args.rhai_dir).start();
    pool.write().unwrap().start(global.clone());

    HttpServer::new(move || {
        App::new()
            .route("/", web::get().to(index))
            .app_data(web::Data::new(global.clone()))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
