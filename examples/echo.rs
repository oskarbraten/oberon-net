use anyhow::Result;
use tokio::time::{sleep, Duration};
use zelda::{Client, Config, Event, Message, Server};

#[tokio::main]
async fn main() -> Result<()> {
    let tcp_address = "127.0.0.1:10000".parse()?;
    let udp_address = "127.0.0.1:10001".parse()?;

    let (server_sender, server_receiver, server_task) =
        Server::listen(tcp_address, udp_address, Config::default());
    tokio::spawn(async move {
        let res = server_task.await;
        println!("Server task result: {:#?}", res);
    });

    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    let (client_sender, client_receiver, client_task) =
        Client::connect(tcp_address, udp_address, Config::default());
    tokio::spawn(async move {
        let res = client_task.await;
        println!("Client task result: {:#?}", res);
    });

    tokio::spawn(async move {
        loop {
            tokio::select! {
                event = server_receiver.recv() => {
                    match event {
                        Ok(event) => {
                            match event {
                                (id, Event::Connected) => {
                                    println!("Client {}, connected!", id);
                                },
                                (id, Event::Received { data }) => {
                                    println!("Client {}, received: {}", id, std::str::from_utf8(&data).unwrap());
                                    server_sender.send(id, Message::reliable(data)).unwrap();
                                },
                                (id, Event::Disconnected) => {
                                    println!("Client {}, disconnected!", id);
                                }
                            }
                        },
                        Err(err) => println!("{}", err)
                    }
                },
                event = client_receiver.recv() => {
                    match event {
                        Ok(event) => {
                            match event {
                                Event::Connected => {
                                    println!("Connected from server!");
                                },
                                Event::Received { data } => {
                                    println!("Received from server: {}", std::str::from_utf8(&data).unwrap());
                                },
                                Event::Disconnected => {
                                    println!("Disconnected from server!");
                                }
                            }
                        },
                        Err(err) => println!("{}", err)
                    }
                },
            }
        }
    });

    loop {
        sleep(Duration::from_secs(1)).await;
        if rand::random::<f32>() > 0.5 {
            println!("Client sent reliable message.");
            client_sender.send(Message::reliable(b"Hello, world!"[..].into()))?;
        } else {
            println!("Client sent unreliable message.");
            client_sender.send(Message::unreliable(b"Hello, world!"[..].into()))?;
        }
    }
}
