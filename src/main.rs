use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        ConnectInfo, State,
    },
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use cfg_if::cfg_if;
use opencv::{
    core::{Size, Vector},
    imgcodecs, imgproc,
    prelude::*,
    videoio,
};
use tokio::{net, sync::broadcast};

type FrameData = Vec<u8>;

const MAX_FRAME_RATE: f32 = 30.;
const IMG_FORMAT: &str = ".webp";

async fn footage_capture(
    footage_tx: Arc<broadcast::Sender<FrameData>>,
    mut cam: videoio::VideoCapture,
) {
    let mut frame = Mat::default();
    let mut resized_frame = Mat::default();

    #[cfg(feature = "bgr2rgb")]
    let mut rgb_frame = Mat::default();

    let mut frame_data = Vector::new();
    let params = {
        let mut p = Vector::<i32>::new();
        p.push(imgcodecs::IMWRITE_WEBP_QUALITY);
        p.push(70);
        p
    };

    let mut start = Instant::now();
    let mut end = Instant::now();

    loop {
        tokio::time::sleep(Duration::from_millis(
            (1000. / MAX_FRAME_RATE - (end - start).as_millis() as f32) as u64,
        ))
        .await;

        if footage_tx.receiver_count() < 1 {
            continue;
        }

        start = Instant::now();

        // Capture camera frame
        cam.read(&mut frame).unwrap();

        // Resize to a smaller image
        imgproc::resize(
            &frame,
            &mut resized_frame,
            Size::new(1024, 576),
            0.0,
            0.0,
            imgproc::INTER_LINEAR,
        )
        .unwrap();

        // Convert color formats and encode image
        cfg_if! {
            if #[cfg(feature = "bgr2rgb")] {
                imgproc::cvt_color(&resized_frame, &mut rgb_frame, imgproc::COLOR_BGR2RGB, 0).unwrap();
                imgcodecs::imencode(IMG_FORMAT, &rgb_frame, &mut frame_data, &params).unwrap();
            } else {
                imgcodecs::imencode(IMG_FORMAT, &resized_frame, &mut frame_data, &params).unwrap();
            }
        }

        // Broadcast to network tasks
        //println!("Frame size: {} bytes", frame_data.len());
        _ = footage_tx.send(frame_data.to_vec());

        end = Instant::now();
    }
}

async fn index() -> impl IntoResponse {
    Html(include_str!("index.html"))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(footage_tx): State<Arc<broadcast::Sender<FrameData>>>,
) -> impl IntoResponse {
    let rx = footage_tx.subscribe();
    ws.on_upgrade(move |sock| client_handler(sock, addr, rx))
}

async fn client_handler(
    mut socket: WebSocket,
    addr: SocketAddr,
    mut footage_rx: broadcast::Receiver<FrameData>,
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
    let cam = videoio::VideoCapture::new(0, videoio::CAP_ANY)?;
    if !cam.is_opened()? {
        panic!("Unable to open camera!");
    }

    // Spawn footage capture task
    let (tx, _) = broadcast::channel::<FrameData>(1);
    let tx = Arc::new(tx);
    tokio::spawn(footage_capture(tx.clone(), cam));

    // Make and serve HTTP server
    let app = Router::new()
        .route("/", get(index))
        .route("/ws", get(ws_handler))
        .with_state(tx);

    let listener = net::TcpListener::bind("0.0.0.0:7020").await?;
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await?;

    Ok(())
}
