use actix::{Actor, StreamHandler};
use actix_web::{HttpServer, App, HttpRequest, web, HttpResponse, Error};
use actix_web_actors::ws;
use clap::Parser;
use dotenvy::dotenv;
use log::{info, debug};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
}

struct WsProt;

impl Actor for WsProt {
    type Context = ws::WebsocketContext<Self>;
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for WsProt {
    fn handle(&mut self, msg: Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
        use ws::Message as M;
        match msg {
            Ok(M::Ping(msg)) => ctx.pong(&msg),
            Ok(M::Text(text)) => ctx.text(text),
            Ok(M::Binary(bin)) => ctx.binary(bin),
            _ => (),
        }
    }
}

async fn index(req: HttpRequest, stream: web::Payload) -> Result<HttpResponse, Error> {
    let resp = ws::start(WsProt {}, &req, stream);
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

    HttpServer::new(|| {
        App::new()
            .route("/", web::get().to(index))
    })
    .bind(("127.0.0.1", 8080))?
    .run()
    .await
}
