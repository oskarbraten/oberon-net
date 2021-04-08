use anyhow::Result;
use std::{future::Future, net::SocketAddr, thread};
use tokio::{
    net::{TcpStream, UdpSocket},
    sync::mpsc::{
        unbounded_channel as tokio_channel, UnboundedReceiver as TokioReceiver,
        UnboundedSender as TokioSender,
    },
};

use crate::utils::{read_frame, sign, verify};
use crate::{Config, Connection, Event, Message};

pub struct Client;

impl Client {
    pub fn connect_with_runtime(
        tcp_address: SocketAddr,
        udp_address: SocketAddr,
        config: Config,
        worker_threads: usize,
    ) -> (ClientSender, ClientReceiver) {
        let (sender, receiver, task) = Self::connect(tcp_address, udp_address, config);

        thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(worker_threads)
                .enable_all()
                .build()
                .unwrap();

            runtime.block_on(task).unwrap();
        });

        (sender, receiver)
    }

    pub fn connect(
        tcp_address: SocketAddr,
        udp_address: SocketAddr,
        config: Config,
    ) -> (
        ClientSender,
        ClientReceiver,
        impl Future<Output = Result<(), anyhow::Error>>,
    ) {
        let (outbound_sender, outbound_receiver) = tokio_channel::<Message>();
        let (inbound_sender, inbound_receiver) =
            async_channel::bounded::<Event>(config.event_capacity);

        let task = Self::task(
            tcp_address,
            udp_address,
            config,
            inbound_sender,
            outbound_receiver,
        );

        (
            ClientSender {
                sender: outbound_sender,
            },
            ClientReceiver {
                receiver: inbound_receiver,
            },
            task,
        )
    }

    async fn task(
        tcp_address: SocketAddr,
        udp_address: SocketAddr,
        _config: Config,
        inbound_sender: async_channel::Sender<Event>,
        mut outbound_receiver: TokioReceiver<Message>,
    ) -> Result<()> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.connect(udp_address).await?;

        let mut socket_buffer: [u8; 1200] = [0; 1200];

        let stream = TcpStream::connect(tcp_address).await?;
        stream.set_nodelay(true).unwrap();
        let (mut read_stream, write_stream) = stream.into_split();

        let (id, mut connection) =
            Connection::connect(&socket, &mut read_stream, write_stream).await?;

        inbound_sender.send(Event::Connected).await?;

        loop {
            tokio::select! {
                result = read_frame(&mut read_stream, 500000) => {
                    match result {
                        Ok(data) => {
                            inbound_sender.send(Event::Received {
                                data
                            }).await?;
                        },
                        Err(err) => {
                            println!("Read frame error: {:#?}", err);
                            inbound_sender.send(Event::Disconnected).await?;
                            break Err(err.into());
                        }
                    }
                },
                result = socket.recv(&mut socket_buffer) => {
                    if let Ok(bytes_read) = result {
                        // Must receive more than tag (u64) bytes
                        if bytes_read > 8 {
                            let tag = &socket_buffer[0..8];
                            let data = &socket_buffer[8..bytes_read];

                            if verify(&connection.key, data, tag) {
                                // Verified sender, create event:
                                inbound_sender.send(Event::Received {
                                    data: data.to_vec()
                                }).await?;
                            }
                        }
                    }
                },
                result = outbound_receiver.recv() => {
                    if let Some(mut message) = result {
                        if message.reliable {
                            match connection.write(&message.data).await {
                                Ok(()) => {},
                                Err(err) => println!("Error writing message (TCP): {}", err)
                            }
                        } else {
                            let mut data = id.to_be_bytes().to_vec(); // Add id.
                            data.extend(&sign(&connection.key, &message.data)); // Add tag.
                            data.append(&mut message.data); // Add data.

                            match socket.send(&data).await {
                                Ok(_) => {},
                                Err(err) => println!("Error writing message (UDP): {}", err)
                            }
                        }
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ClientSender {
    sender: TokioSender<Message>,
}

impl ClientSender {
    pub fn send(&self, message: Message) -> Result<()> {
        self.sender.send(message).map_err(|err| err.into())
    }
}

#[derive(Debug, Clone)]
pub struct ClientReceiver {
    receiver: async_channel::Receiver<Event>,
}

impl ClientReceiver {
    pub async fn recv(&self) -> Result<Event> {
        self.receiver.recv().await.map_err(|err| err.into())
    }

    pub fn try_recv(&self) -> Result<Event> {
        self.receiver.try_recv().map_err(|err| err.into())
    }
}
