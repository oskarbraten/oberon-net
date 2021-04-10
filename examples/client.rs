use anyhow::Result;
use rcgen::generate_simple_self_signed;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio_rustls::{rustls::ClientConfig, webpki::DNSNameRef};

use zelda::{Client, Config, Event};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let generated_certificate = generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();

    let config = {
        let mut config = ClientConfig::new();
        config
            .root_store
            .add_pem_file(&mut generated_certificate.serialize_pem()?.as_bytes())
            .unwrap();
        config
    };

    let domain = DNSNameRef::try_from_ascii_str("localhost")
        .unwrap()
        .to_owned();

    let address = "localhost:10000";

    let (sender, receiver, task) = Client::connect(address, Config::default(), domain, config);

    tokio::spawn(async move {
        task.await.unwrap();
    });

    tokio::spawn(async move {
        loop {
            match receiver.recv().await {
                Ok(event) => match event {
                    Event::Connected => {
                        log::info!("Connected to server!");

                        tokio::spawn(async move {});
                    }
                    Event::Received(data) => {
                        log::info!(
                            "Received from server: {}",
                            std::str::from_utf8(&data).unwrap()
                        );
                    }
                    Event::Disconnected => {
                        log::info!("Disconnected from server!");
                    }
                },
                Err(err) => {
                    log::debug!("{}", err);
                    break;
                }
            }
        }
    });

    let stdin = tokio::io::stdin();
    let reader = BufReader::new(stdin);
    let mut stream = reader.lines();

    println!("Enter message: ");
    while let Ok(Some(line)) = stream.next_line().await {
        sender.reliable(line.into_bytes())?;
        println!("Enter message: ");
    }

    Ok(())
}
