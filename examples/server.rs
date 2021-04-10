use anyhow::Result;
use rcgen::generate_simple_self_signed;
use tokio_rustls::rustls::{
    internal::pemfile::{certs, pkcs8_private_keys},
    NoClientAuth, ServerConfig,
};
use zelda::{Config, Event, Server};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let address = "127.0.0.1:10000";

    let generated_certificate = generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();

    let config = {
        let mut config = ServerConfig::new(NoClientAuth::new());
        let certificates = certs(&mut generated_certificate.serialize_pem()?.as_bytes()).unwrap();

        let keys =
            pkcs8_private_keys(&mut generated_certificate.serialize_private_key_pem().as_bytes())
                .unwrap();

        config.set_single_cert(certificates, keys[0].clone())?;
        config
    };

    let (sender, receiver, task) = Server::listen(address, Config::default(), config);

    tokio::try_join!(
        tokio::spawn(async move {
            task.await.unwrap();
        }),
        tokio::spawn(async move {
            loop {
                match receiver.recv().await {
                    Ok(event) => match event {
                        (id, Event::Connected) => {
                            log::info!("SERVER: Client {}, connected!", id);
                        }
                        (id, Event::Received(data)) => {
                            log::info!(
                                "SERVER: received: {} (Connection id: {})",
                                std::str::from_utf8(&data).unwrap(),
                                id,
                            );

                            let mut data = data;
                            data.extend(b"- ECHO.");
                            sender.reliable(id, data).unwrap();
                        }
                        (id, Event::Disconnected) => {
                            log::info!("SERVER: Client {}, disconnected!", id);
                        }
                    },
                    Err(err) => {
                        log::debug!("{}", err);
                        break;
                    }
                }
            }
        })
    )
    .unwrap();

    Ok(())
}
