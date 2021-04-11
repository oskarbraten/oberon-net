use anyhow::Result;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_rustls::{rustls::ClientConfig, webpki::DNSNameRef};

use zelda::{Client, Config, Event};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let config = {
        let mut config = ClientConfig::new();

        let file = std::fs::File::open("./examples/certs/cert.pem")?;
        let mut buffered = std::io::BufReader::new(file);

        config.root_store.add_pem_file(&mut buffered).unwrap();
        config
    };

    let domain = DNSNameRef::try_from_ascii_str("localhost")
        .unwrap()
        .to_owned();

    let address = "localhost:10000";

    let (sender, mut receiver, task) = Client::connect(
        address,
        Config::default(),
        domain,
        config,
        b"TOKEN".to_vec(),
    );

    tokio::spawn(async move {
        task.await.unwrap();
    });

    loop {
        match receiver.recv().await {
            Some(event) => match event {
                Event::Connected => {
                    println!("Connected to server!");

                    let sender = sender.clone();
                    tokio::spawn(async move {
                        let stdin = tokio::io::stdin();
                        let reader = BufReader::new(stdin);
                        let mut stream = reader.lines();

                        loop {
                            println!("Choose message type (and press Enter):");
                            println!(" 1. Reliable (1)");
                            println!(" 2. Unreliable (2)");

                            let line = stream.next_line().await.ok().flatten().unwrap();
                            match line.parse::<u8>().ok().filter(|t| *t == 1 || *t == 2) {
                                Some(message_type) => {
                                    println!("Enter message: ");

                                    let line = stream.next_line().await.ok().flatten().unwrap();
                                    if message_type == 1 {
                                        sender.reliable(line.into_bytes()).unwrap();
                                    } else {
                                        sender.unreliable(line.into_bytes()).unwrap();
                                    }
                                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                                }
                                None => {
                                    println!("Invalid message type, try again.");
                                    continue;
                                }
                            }
                        }
                    });
                }
                Event::Received(data) => {
                    println!(
                        "Received from server: {}",
                        std::str::from_utf8(&data).unwrap()
                    );
                }
                Event::Disconnected => {
                    println!("Disconnected from server!");
                }
            },
            None => {
                log::debug!("Receiver returned none.");
                break;
            }
        }
    }

    Ok(())
}
