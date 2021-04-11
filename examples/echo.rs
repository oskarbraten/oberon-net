use anyhow::Result;
use tokio::time::{sleep, Duration};
use tokio_rustls::{
    rustls::{
        internal::pemfile::{certs, pkcs8_private_keys},
        ClientConfig, NoClientAuth, ServerConfig,
    },
    webpki::DNSNameRef,
};
use zelda::{Client, Config, Event, Server};

fn main() -> Result<()> {
    env_logger::init();

    let address = "127.0.0.1:10000";

    let server_config = {
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

    let client_config = {
        let mut config = ClientConfig::new();

        let file = std::fs::File::open("./examples/certs/cert.pem")?;
        let mut buffered = std::io::BufReader::new(file);

        config.root_store.add_pem_file(&mut buffered).unwrap();
        config
    };

    let client_domain = DNSNameRef::try_from_ascii_str("localhost")
        .unwrap()
        .to_owned();

    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let correct_token = b"TOKEN".to_vec();
            let (server_sender, mut server_receiver, server_task) =
                Server::listen(address, Config::default(), server_config, move |token| {
                    let opt = if token == correct_token {
                        Some("Hunter2")
                    } else {
                        None
                    };

                    async move { opt }
                });
            let (r1, r2) = tokio::join!(
                tokio::spawn(server_task),
                tokio::spawn(async move {
                    loop {
                        match server_receiver.recv().await {
                            Some(event) => match event {
                                (id, Event::Connected, _info) => {
                                    log::info!("SERVER: Client {}, connected!", id);
                                }
                                (id, Event::Received(data), _info) => {
                                    log::info!(
                                        "SERVER: received: {} (Connection id: {})",
                                        std::str::from_utf8(&data).unwrap(),
                                        id,
                                    );

                                    let mut data = data;
                                    data.extend(b"- ECHO.");
                                    server_sender.reliable(id, data).unwrap();
                                }
                                (id, Event::Disconnected, _info) => {
                                    log::info!("SERVER: Client {}, disconnected!", id);
                                }
                            },
                            None => {
                                log::debug!("SERVER: Receiver returned none.");
                                break;
                            }
                        }
                    }
                })
            );

            r1.unwrap().unwrap();
            r2.unwrap();
        });
    });

    std::thread::sleep(Duration::from_millis(500));

    let t2 = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (client_sender, mut client_receiver, client_task) = Client::connect(
                address,
                Config::default(),
                client_domain,
                client_config,
                b"TOKEN".to_vec(),
            );

            client_sender
                .reliable(b"This message was sent before being connected.".to_vec())
                .unwrap();

            tokio::select! {
                _ = sleep(Duration::from_millis(5000)) => {},
                result = client_task => {
                    log::info!("CLIENT: Task completed with result: {:#?}", result);
                },
                _ = tokio::spawn(async move {
                    loop {
                        match client_receiver.recv().await {
                            Some(event) => match event {
                                Event::Connected => {
                                    log::info!("CLIENT: Connected to server!");

                                    let client_sender = client_sender.clone();
                                    tokio::spawn(async move {
                                        loop {
                                            tokio::time::sleep(Duration::from_millis(500)).await;
                                            if rand::random::<f32>() > 0.5 {
                                                client_sender
                                                    .reliable(b"Hello, world!".to_vec())
                                                    .unwrap();
                                            } else {
                                                client_sender
                                                    .unreliable(b"Hello, world!".to_vec())
                                                    .unwrap();
                                            }
                                        }
                                    });
                                }
                                Event::Received(data) => {
                                    log::info!(
                                        "CLIENT: Received from server: {}",
                                        std::str::from_utf8(&data).unwrap()
                                    );
                                }
                                Event::Disconnected => {
                                    log::info!("CLIENT: Disconnected from server!");
                                }
                            },
                            None => {
                                log::debug!("CLIENT: Receiver returned none.");
                                break;
                            }
                        }
                    }
                }) => {}
            }
        });
    });

    t2.join().unwrap();

    Ok(())
}
