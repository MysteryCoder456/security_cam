use std::{
    io::Cursor,
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
use drm_fourcc::DrmFourcc;
use image::{ImageBuffer, ImageFormat, RgbImage};
use libcamera::{
    camera::CameraConfigurationStatus,
    camera_manager::CameraManager,
    framebuffer_allocator::{FrameBuffer, FrameBufferAllocator},
    framebuffer_map::MemoryMappedFrameBuffer,
    geometry::Size,
    pixel_format::PixelFormat,
    request::ReuseFlag,
    stream::StreamRole,
};
use rppal::pwm;
use tokio::{
    net,
    sync::{broadcast, mpsc},
};

type FrameData = Vec<u8>;

enum ServoPosition {
    Left,
    Center,
    Right,
    Custom(f32),
}

const MAX_FRAME_RATE: f32 = 30.;
const PIXEL_FORMAT: PixelFormat = PixelFormat::new(DrmFourcc::Bgr888 as u32, 0);
const MAX_PULSE_WIDTH_US: u64 = 2000;
const MIN_PULSE_WIDTH_US: u64 = 1000;

fn footage_capture_task(footage_tx: Arc<broadcast::Sender<FrameData>>) {
    // Initialize camera
    let cam_mgr = CameraManager::new().unwrap();
    let cameras = cam_mgr.cameras();
    let cam = cameras.get(0).unwrap();
    let mut cam = cam.acquire().unwrap();
    let size = Size {
        width: 1024,
        height: 768,
    };

    // Configure camera
    let mut cfgs = cam
        .generate_configuration(&[StreamRole::VideoRecording])
        .unwrap();
    cfgs.get_mut(0).unwrap().set_pixel_format(PIXEL_FORMAT);
    cfgs.get_mut(0).unwrap().set_size(size);

    match cfgs.validate() {
        CameraConfigurationStatus::Valid => println!("Camera configuration valid!"),
        CameraConfigurationStatus::Adjusted => {
            println!("Camera configuration was adjusted: {:#?}", cfgs)
        }
        CameraConfigurationStatus::Invalid => panic!("Error validating camera configuration"),
    }

    cam.configure(&mut cfgs).unwrap();

    // Allocate framebuffers
    let mut alloc = FrameBufferAllocator::new(&cam);
    let cfg = cfgs.get(0).unwrap();
    let stream = cfg.stream().unwrap();
    let buffers = alloc.alloc(&stream).unwrap();

    let buffers = buffers
        .into_iter()
        .map(|buf| MemoryMappedFrameBuffer::new(buf).unwrap())
        .collect::<Vec<_>>();

    // Create requests and bind the allocated buffers
    let reqs = buffers
        .into_iter()
        .enumerate()
        .map(|(i, buf)| {
            let mut req = cam.create_request(Some(i as u64)).unwrap();
            req.add_buffer(&stream, buf).unwrap();
            req
        })
        .collect::<Vec<_>>();

    let (req_tx, req_rx) = std::sync::mpsc::channel();
    cam.on_request_completed(move |req| {
        req_tx.send(req).unwrap();
    });

    // Start camera and queue capture requests
    cam.start(None).unwrap();
    for req in reqs {
        cam.queue_request(req).unwrap();
    }

    let mut converted_bytes = Vec::new();

    let mut start = Instant::now();
    let mut end = Instant::now();

    loop {
        std::thread::sleep(Duration::from_millis(
            (1000. / MAX_FRAME_RATE - (end - start).as_millis() as f32) as u64,
        ));

        if footage_tx.receiver_count() < 1 {
            continue;
        }

        start = Instant::now();

        let mut req = req_rx.recv().unwrap();

        let framebuffer: &MemoryMappedFrameBuffer<FrameBuffer> = req.buffer(&stream).unwrap();
        let planes = framebuffer.data();

        let frame_data = planes.get(0).unwrap();

        let img: RgbImage =
            ImageBuffer::from_raw(size.width, size.height, frame_data.to_vec()).unwrap();
        img.write_to(&mut Cursor::new(&mut converted_bytes), ImageFormat::Jpeg)
            .unwrap();

        _ = footage_tx.send(converted_bytes.clone());

        req.reuse(ReuseFlag::REUSE_BUFFERS);
        cam.queue_request(req).unwrap();

        end = Instant::now();
    }
}

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
