use cfg_if::cfg_if;
use opencv::{
    core::{Size, Vector},
    imgcodecs, imgproc,
    prelude::*,
    videoio,
};
use std::time::Duration;
use tokio::{io::AsyncWriteExt, net, sync::broadcast};

type FrameData = Vec<u8>;

const MAX_FRAME_RATE: f32 = 30.;
const IMG_FORMAT: &str = ".webp";

async fn client_connection(
    mut footage_rx: broadcast::Receiver<FrameData>,
    mut stream: net::TcpStream,
) {
    loop {
        // Receive broadcasted footage data...
        let footage_data = match footage_rx.recv().await {
            Ok(data) => data,
            Err(e) => {
                eprintln!("{e:?}");
                continue;
            }
        };

        // ...and send it down to client
        match stream.write_all(&footage_data).await {
            Ok(()) => {}
            Err(_) => {
                println!("Client disconnected");
                break;
            }
        }
    }
}

async fn footage_capture(footage_tx: broadcast::Sender<FrameData>, mut cam: videoio::VideoCapture) {
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

    loop {
        tokio::time::sleep(Duration::from_millis((1000. / MAX_FRAME_RATE) as u64)).await;

        if footage_tx.receiver_count() < 1 {
            continue;
        }

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
        footage_tx.send(frame_data.to_vec()).unwrap();
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cam = videoio::VideoCapture::new(0, videoio::CAP_ANY)?;
    if !cam.is_opened()? {
        panic!("Unable to open camera!");
    }

    let listener = net::TcpListener::bind("0.0.0.0:7020").await?;
    let (tx, _) = broadcast::channel::<FrameData>(1);

    // Spawn data broadcast task
    let tx_clone = tx.clone();
    tokio::spawn(footage_capture(tx_clone, cam));

    loop {
        let (sock, addr) = listener.accept().await?;
        println!("New connection from {:?}", addr);

        // Spawn new connection task
        let rx = tx.subscribe();
        tokio::spawn(client_connection(rx, sock));
    }
}
