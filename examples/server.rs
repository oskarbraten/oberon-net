use anyhow::Result;
use tokio_rustls::rustls::{
    internal::pemfile::{certs, pkcs8_private_keys},
    NoClientAuth, ServerConfig,
};
use zelda::{Config, Server, ServerEvent};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let address = "127.0.0.1:10000";

    let config = {
        let mut config = ServerConfig::new(NoClientAuth::new());

        let certificates = {
            let file = std::fs::File::open("./examples/certs/cert.pem")?;
            let mut buffered = std::io::BufReader::new(file);

            certs(&mut buffered).unwrap()
        };

        let key = {
            let file = std::fs::File::open("./examples/certs/key.pem")?;
            let mut buffered = std::io::BufReader::new(file);

            pkcs8_private_keys(&mut buffered).unwrap()[0].clone()
        };

        config.set_single_cert(certificates, key)?;
        config
    };

    let correct_token = b"TOKEN".to_vec();

    let (sender, mut receiver, task) =
        Server::listen(address, Config::default(), config, move |token| {
            if token == correct_token {
                Some("Hunter2")
            } else {
                None
            }
        });

    tokio::try_join!(
        tokio::spawn(async move {
            println!("Listening on {}", address);
            task.await.unwrap();
        }),
        tokio::spawn(async move {
            loop {
                match receiver.recv().await {
                    Some(event) => match event {
                        ServerEvent::Connected { id, claim: _ } => {
                            println!("SERVER - Client {}, connected!", id);
                        }
                        ServerEvent::Received { id, data } => {
                            println!(
                                "SERVER - Received from client ({}): {}",
                                id,
                                std::str::from_utf8(&data).unwrap(),
                            );

                            let mut data = data;
                            data.extend(b" - seen by server.");
                            sender.reliable(id, data).unwrap();
                        }
                        ServerEvent::Disconnected { id } => {
                            println!("SERVER - Client {}, disconnected!", id);
                        }
                    },
                    None => {
                        log::debug!("Receiver returned none.");
                        break;
                    }
                }
            }
        })
    )
    .unwrap();

    Ok(())
}
