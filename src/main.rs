use opencv::{
    core::{Size, Vector},
    imgcodecs, imgproc,
    prelude::*,
    videoio,
};
use std::time::Duration;
use tokio::{io::AsyncWriteExt, net, sync::broadcast};

type FrameData = Vec<u8>;

async fn broadcast_connection(
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let mut cam = videoio::VideoCapture::new(0, videoio::CAP_ANY)?;
    if !cam.is_opened()? {
        panic!("Unable to open camera!");
    }

    let listener = net::TcpListener::bind("0.0.0.0:7020").await?;
    let (tx, _) = broadcast::channel::<FrameData>(1);

    // Spawn data broadcast task
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        let mut frame = Mat::default();
        let mut resized_frame = Mat::default();
        let mut rgb_frame = Mat::default();

        loop {
            tokio::time::sleep(Duration::from_millis(30)).await;

            if tx_clone.receiver_count() < 1 {
                continue;
            }

            // Capture camera frame
            cam.read(&mut frame).unwrap();

            // Resize to a smaller image
            imgproc::resize(
                &frame,
                &mut resized_frame,
                Size::new(1280, 720),
                0.0,
                0.0,
                imgproc::INTER_LINEAR,
            )
            .unwrap();

            // Convert color formats
            imgproc::cvt_color(&resized_frame, &mut rgb_frame, imgproc::COLOR_BGR2RGB, 0).unwrap();

            // Encode image to JPEG data
            let mut frame_data = Vector::new();
            let mut params = Vector::<i32>::new();
            params.push(imgcodecs::IMWRITE_JPEG_QUALITY);
            params.push(70);
            imgcodecs::imencode(".jpg", &rgb_frame, &mut frame_data, &params).unwrap();

            // Broadcast to network tasks
            tx_clone.send(frame_data.to_vec()).unwrap();
        }
    });

    loop {
        let (sock, addr) = listener.accept().await?;
        println!("New connection from {:?}", addr);

        // Spawn new connection task
        let rx = tx.subscribe();
        tokio::spawn(broadcast_connection(rx, sock));
    }
}
