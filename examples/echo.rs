use anyhow::Result;
use rcgen::generate_simple_self_signed;
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
    let mut builder = env_logger::Builder::from_default_env();
    builder.target(env_logger::Target::Stdout);

    builder.init();

    let generated_certificate = generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();

    let server_config = {
        let mut config = ServerConfig::new(NoClientAuth::new());
        let certificates = certs(&mut generated_certificate.serialize_pem()?.as_bytes()).unwrap();

        let keys =
            pkcs8_private_keys(&mut generated_certificate.serialize_private_key_pem().as_bytes())
                .unwrap();

        config.set_single_cert(certificates, keys[0].clone())?;
        config
    };

    let client_config = {
        let mut config = ClientConfig::new();
        config
            .root_store
            .add_pem_file(&mut generated_certificate.serialize_pem()?.as_bytes())
            .unwrap();
        config
    };

    let client_domain = DNSNameRef::try_from_ascii_str("localhost")
        .unwrap()
        .to_owned();

    let tcp_address = "127.0.0.1:10000".parse()?;
    let udp_address = "127.0.0.1:10001".parse()?;

    let t1 = std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let (server_sender, server_receiver, server_task) =
                Server::listen(tcp_address, udp_address, Config::default(), server_config);
            let (r1, r2) = tokio::join!(
                tokio::spawn(server_task),
                tokio::spawn(async move {
                    loop {
                        match server_receiver.recv().await {
                            Ok(event) => match event {
                                (id, Event::Connected) => {
                                    log::info!("Client {}, connected!", id);

                                    let server_sender = server_sender.clone();
                                    tokio::spawn(async move {
                                        loop {
                                            sleep(Duration::from_millis(150)).await;
                                            server_sender
                                                .reliable(id, b"Hello, client!".to_vec())
                                                .unwrap();
                                        }
                                    });
                                }
                                (id, Event::Received { data }) => {
                                    log::info!(
                                        "Client {}, received: {}",
                                        id,
                                        std::str::from_utf8(&data).unwrap()
                                    );

                                    let mut data = data;
                                    data.extend(b"- ECHO.");
                                    server_sender.reliable(id, data).unwrap();
                                }
                                (id, Event::Disconnected) => {
                                    log::info!("Client {}, disconnected!", id);
                                }
                            },
                            Err(err) => {
                                log::debug!("{}", err);
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
            let (client_sender, client_receiver, client_task) = Client::connect(
                tcp_address,
                udp_address,
                Config::default(),
                client_config,
                client_domain,
            );

            match tokio::try_join!(
                tokio::spawn(client_task),
                tokio::spawn(async move {
                    loop {
                        match client_receiver.recv().await {
                            Ok(event) => match event {
                                Event::Connected => {
                                    log::info!("Connected to server!");
                                }
                                Event::Received { data } => {
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
                }),
                tokio::spawn(async {
                    sleep(Duration::from_millis(5000)).await;
                    panic!("Stopping client.")
                })
            ) {
                Ok((r1, _, _)) => {
                    r1.unwrap();
                }
                Err(err) => {
                    log::debug!("Error: {}", err);
                }
            }
        });
    });

    t1.join().unwrap();
    t2.join().unwrap();

    Ok(())
}
