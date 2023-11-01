pub mod proto;
pub mod threadpool;
pub mod utils;
pub mod ws;

use std::sync::{Arc, RwLock};

use actix::{Actor, Addr};
use actix_web::{HttpServer, App, HttpRequest, web, HttpResponse, Error};
use actix_web_actors::ws::start as ws_start;
use clap::Parser;
use dotenvy::dotenv;
use log::debug;
use rand::Rng;
use threadpool::RunnerPool;

use crate::ws::{Global, Local};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
}

async fn index(req: HttpRequest, stream: web::Payload, global: web::Data<Addr<Global>>) -> Result<HttpResponse, Error> {
    // Collision chance (64-bit): 1/18446744073709551615
    // 32-bit: 1/4294967296
    let id = rand::thread_rng().gen::<u32>();
    let resp = ws_start(Local(global, id), &req, stream);
    debug!("{:?}", resp);
    resp
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    Cli::parse();
    dotenv().ok();
    if cfg!(debug_assertions) {
        env_logger::init_from_env(env_logger::Env::new().default_filter_or("debug"));
    } else {
        env_logger::init_from_env(env_logger::Env::new().default_filter_or("info"));
    }

    let pool = new_shared!(RunnerPool::new(8));
    let global = Global::new(Arc::clone(&pool)).start();
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
