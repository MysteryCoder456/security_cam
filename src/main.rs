use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, State,
    },
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use tokio::{
    net,
    sync::{broadcast, mpsc},
};
//use tower_http::compression::CompressionLayer;

use crate::footage::{footage_capture_task, FrameData};
use crate::servo::{initilize_servo, servo_task, ServoPosition};

mod footage;
mod servo;

async fn index() -> impl IntoResponse {
    Html(include_str!("index.html"))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State((footage_tx, servo_tx)): State<(
        Arc<broadcast::Sender<FrameData>>,
        mpsc::UnboundedSender<ServoPosition>,
    )>,
) -> impl IntoResponse {
    let footage_rx = footage_tx.subscribe();
    let servo_tx = servo_tx.clone();
    ws.on_upgrade(move |sock| client_handler(sock, addr, footage_rx, servo_tx))
}

async fn client_handler(
    mut socket: WebSocket,
    addr: SocketAddr,
    mut footage_rx: broadcast::Receiver<FrameData>,
    servo_tx: mpsc::UnboundedSender<ServoPosition>,
) {
    println!("New connection from {:?}", addr);

    loop {
        tokio::select! {
            Ok(footage_data) = footage_rx.recv() => {
                match socket.send(Message::Binary(footage_data.into())).await {
                    Ok(()) => {}
                    Err(e) => {
                        eprintln!("Client {addr:?}: {e:?}");
                        break;
                    }
                }
            },
            Some(Ok(msg)) = socket.recv() => {
                match msg {
                    Message::Text(msg) => {
                        let servo_pos = match msg.as_str() {
                            "left" => Some(ServoPosition::Left),
                            "center" => Some(ServoPosition::Center),
                            "right" => Some(ServoPosition::Right),
                            _ => msg.parse::<f32>().ok().map(|pos| ServoPosition::Custom(pos.clamp(-1.0, 1.0))),
                        };

                        if let Some(servo_pos) = servo_pos {
                            servo_tx.send(servo_pos).unwrap();
                        }
                    },
                    Message::Close(_) => break,
                    _ => {}
                }
            }
        }
    }

    println!("Client {addr:?} disconnected");
}

#[tokio::main(flavor = "multi_thread")]
async fn main() -> anyhow::Result<()> {
    // Initialize PWM servo
    let servo_pwm = initilize_servo()?;

    // Spawn footage capture task
    let (footage_tx, _) = broadcast::channel::<FrameData>(1);
    let footage_tx = Arc::new(footage_tx);
    let footage_tx_clone = footage_tx.clone();
    tokio::task::spawn_blocking(move || {
        footage_capture_task(footage_tx_clone);
    });

    // Spawn servo movement task
    let (servo_tx, servo_rx) = mpsc::unbounded_channel::<ServoPosition>();
    tokio::task::spawn(servo_task(servo_rx, servo_pwm));

    // Make and serve HTTP server
    let app = Router::new()
        //.layer(CompressionLayer::new().zstd(true))
        .route("/", get(index))
        .route("/ws", get(ws_handler))
        .with_state((footage_tx, servo_tx));

    let listener = net::TcpListener::bind("0.0.0.0:7020").await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
