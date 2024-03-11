use opencv::{core::Vector, imgcodecs, prelude::*, videoio};
use std::time::Duration;
use tokio::{io::AsyncWriteExt, net, sync::broadcast, time::sleep};

type Footage = Vec<u8>;

async fn broadcast_connection(
    mut footage_rx: broadcast::Receiver<Footage>,
    mut stream: net::TcpStream,
) {
    loop {
        // Receive broadcasted footage data...
        let footage_data = match footage_rx.recv().await {
            Ok(data) => data,
            Err(e) => {
                println!("{e:?}");
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
    let (tx, _) = broadcast::channel::<Footage>(1);

    // Spawn data broadcast task
    let tx_clone = tx.clone();
    tokio::spawn(async move {
        let mut frame = Mat::default();

        loop {
            // Sleep for 33ms to achieve ~30fps
            sleep(Duration::from_millis(33)).await;

            // Broadcast captured frame to network tasks
            if tx_clone.receiver_count() > 0 {
                cam.read(&mut frame).unwrap();

                let mut frame_data = Vector::new();
                let mut params = Vector::<i32>::new();
                params.push(imgcodecs::IMWRITE_JPEG_QUALITY);
                params.push(70);

                imgcodecs::imencode(".jpg", &frame, &mut frame_data, &Vector::new()).unwrap();
                tx_clone.send(frame_data.to_vec()).unwrap();
            }
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
