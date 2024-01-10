use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net,
};

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let listener = net::TcpListener::bind("0.0.0.0:7020").await?;

    loop {
        let (mut sock, addr) = listener.accept().await?;
        println!("New connection from {:?}", addr);

        let mut buf = vec![0; 1024];
        let bytes_read = sock.read(&mut buf).await?;

        println!("Echoing {} bytes back", bytes_read);
        sock.write(&buf).await?;
    }
}
