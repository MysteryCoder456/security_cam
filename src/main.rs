use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, State,
    },
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use rppal::pwm;
use tokio::{
    net,
    sync::{broadcast, mpsc},
};

use crate::footage::{footage_capture_task, FrameData};

mod footage;

enum ServoPosition {
    Left,
    Center,
    Right,
    Custom(f32),
}

const MAX_PULSE_WIDTH_US: u64 = 2000;
const MIN_PULSE_WIDTH_US: u64 = 1000;

async fn servo_task(mut servo_rx: mpsc::UnboundedReceiver<ServoPosition>, servo_pwm: pwm::Pwm) {
    loop {
        while let Some(servo_pos) = servo_rx.recv().await {
            let pulse_width = match servo_pos {
                ServoPosition::Left => MAX_PULSE_WIDTH_US,
                ServoPosition::Center => (MAX_PULSE_WIDTH_US + MIN_PULSE_WIDTH_US) / 2,
                ServoPosition::Right => MIN_PULSE_WIDTH_US,
                ServoPosition::Custom(pos) => {
                    ((pos * (MAX_PULSE_WIDTH_US - MIN_PULSE_WIDTH_US) as f32 / 2.)
                        + (MAX_PULSE_WIDTH_US + MIN_PULSE_WIDTH_US) as f32 / 2.)
                        as u64
                }
            };
            servo_pwm
                .set_pulse_width(Duration::from_micros(pulse_width))
                .unwrap();
        }
    }
}

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
                match socket.send(Message::Binary(footage_data)).await {
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
    let servo_pwm = pwm::Pwm::with_period(
        pwm::Channel::Pwm0,
        Duration::from_millis(20),
        Duration::from_micros((MAX_PULSE_WIDTH_US + MIN_PULSE_WIDTH_US) / 2),
        pwm::Polarity::Normal,
        true,
    )?;

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
