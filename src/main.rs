use opencv::{highgui, prelude::*, videoio};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net,
};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let window = "camera";
    highgui::named_window(window, highgui::WINDOW_AUTOSIZE)?;
    let mut cam = videoio::VideoCapture::new(0, videoio::CAP_ANY)?;

    if !cam.is_opened()? {
        panic!("Unable to open camera!");
    }

    //let listener = net::TcpListener::bind("0.0.0.0:7020").await?;

    loop {
        //let (mut sock, addr) = listener.accept().await?;
        //println!("New connection from {:?}", addr);

        //let mut buf = vec![0; 1024];
        //let bytes_read = sock.read(&mut buf).await?;

        //println!("Echoing {} bytes back", bytes_read);
        //sock.write(&buf).await?;

        let mut frame = Mat::default();
        cam.read(&mut frame)?;
        if frame.size()?.width > 0 {
            highgui::imshow(window, &frame)?;
        }

        let key = highgui::wait_key(10)?;
        if key > 0 && key != 255 {
            break;
        }
    }

    Ok(())
}
