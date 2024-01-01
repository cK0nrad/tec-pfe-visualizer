extern crate serde_json;

use axum::routing::get;
use axum::Router;
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    response::Response,
};
use byteorder::{ByteOrder, LittleEndian};
use serde_json::json;

use std::io::Read;
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, RwLock};
use std::thread::{self, sleep};
use std::time::Duration;
use tower_http::services::ServeDir;

struct Store {
    position: RwLock<(f64, f64)>,
    girouette: RwLock<String>,
}
impl Store {
    fn new() -> Self {
        Self {
            position: RwLock::new((0.0, 0.0)),
            girouette: RwLock::new(String::from("")),
        }
    }

    fn get_position(&self) -> (f64, f64) {
        *self.position.read().unwrap()
    }

    fn get_girouette(&self) -> String {
        self.girouette.read().unwrap().clone()
    }
    fn set_position(&self, x: f64, y: f64) {
        *self.position.write().unwrap() = (x, y);
    }
    fn set_girouette(&self, girouette: String) {
        *self.girouette.write().unwrap() = girouette;
    }

    fn to_json(&self) -> String {
        let (lat, lon) = self.get_position();
        let girouette = self.get_girouette();
        let json = json!( {
            "lat": lat,
            "lon": lon,
            "girouette": girouette
        });
        json.to_string()
    }
}

#[tokio::main]
async fn main() {
    let store = Store::new();
    let store = Arc::new(store);

    let th_store = Arc::clone(&store);
    tokio::spawn(async move {
        println!("Starting server...");
        let th = th_store.clone();
        match reader(th) {
            Ok(_) => println!("Server stopped"),
            Err(e) => eprintln!("Server error: {}", e),
        }
    });

    let app = Router::new()
        .nest_service("/", ServeDir::new("assets"))
        .route("/ws", get(handler))
        .with_state(Arc::clone(&store));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn handler(ws: WebSocketUpgrade, State(app): State<Arc<Store>>) -> Response {
    ws.on_upgrade(move |socket| async {
        handle_socket(socket, app).await;
    })
}

async fn handle_socket(mut socket: WebSocket, store: Arc<Store>) {
    loop {
        let msg = store.to_json();

        match socket.send(Message::Text(msg)).await {
            Ok(_) => {}
            Err(e) => {
                eprintln!("Error: {}", e);
                break;
            }
        };
        sleep(Duration::from_secs(1));
    }
}

fn handle_client(mut stream: TcpStream, store: Arc<Store>) {
    loop {
        let mut buffer = [0; 1024];
        match stream.read(&mut buffer) {
            Ok(0) => {
                println!("Client closed the connection");
                break;
            }
            Ok(size) => {
                if size >= 8 {
                    let lat = LittleEndian::read_f64(&buffer[0..8]);
                    let lon = LittleEndian::read_f64(&buffer[8..16]);
                    store.set_position(lat, lon);

                    if size > 8 {
                        let string_data = &buffer[16..size];
                        match String::from_utf8(string_data.to_vec()) {
                            Ok(s) => store.set_girouette(s),
                            Err(e) => eprintln!("Failed to read string: {}", e),
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to read from socket: {}", e);
                break;
            }
        }
    }
}

fn reader(store: Arc<Store>) -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8080")?;
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let clone = Arc::clone(&store);
                thread::spawn(move || handle_client(stream, clone));
            }
            Err(e) => {
                eprintln!("Error: {}", e);
            }
        }
    }
    Ok(())
}
