use aes::Aes128;
use anyhow::Result;
use cmac::{Cmac, Mac, NewMac};
use rand::RngCore;
use std::{convert::TryInto, net::SocketAddr};
use tokio::{
    io,
    io::AsyncWriteExt,
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        UdpSocket,
    },
    time::{sleep, Duration},
};

use crate::utils::read_frame;

#[derive(Debug)]
pub struct Connection {
    pub key: [u8; 16],
    pub mac: Cmac<Aes128>,
    pub write_stream: OwnedWriteHalf,
    pub udp_address: Option<SocketAddr>,
}

impl Connection {
    pub async fn connect(
        socket: &UdpSocket,
        read_stream: &mut OwnedReadHalf,
        write_stream: OwnedWriteHalf,
    ) -> Result<(u32, Self)> {
        let data = read_frame(read_stream, 2500).await?;
        let id = u32::from_be_bytes(data[0..4].try_into()?);
        let key: [u8; 16] = data[4..20].try_into()?;

        let mut mac = Cmac::<Aes128>::new_varkey(&key).map_err(|err| anyhow::anyhow!("{}", err))?;

        let tag: [u8; 8] = {
            mac.update(b"ACK");

            mac.finalize_reset().into_bytes().as_slice()[0..8]
                .try_into()
                .unwrap()
        };

        let mut ack = data[0..4].to_vec(); // Add id (raw received bytes).
        ack.extend(&tag); // Add tag.
        ack.extend(b"ACK"); // Add data.

        // Handshake - Send ACK (2):
        loop {
            tokio::select! {
                result = read_frame(read_stream, 80) => {
                    let data = result?;
                    if data == b"ACK" {
                        break;
                    }
                },
                _ = sleep(Duration::from_millis(100)) => {
                    socket.send(&ack).await?;
                }
            }
        }

        Ok((
            id,
            Self {
                key,
                mac,
                write_stream,
                udp_address: None,
            },
        ))
    }

    pub async fn accept(id: u32, mut write_stream: OwnedWriteHalf) -> Result<Self> {
        let mut key = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut key);

        let mac = Cmac::<Aes128>::new_varkey(&key).map_err(|err| anyhow::anyhow!("{}", err))?;

        // Handshake - Initiate (1):
        write_stream.write_u32(4 + key.len() as u32).await?; // id (u32) size + key length
        write_stream.write_u32(id as u32).await?;
        write_stream.write(&key).await?;

        Ok(Self {
            key,
            mac,
            write_stream,
            udp_address: None,
        })
    }

    pub async fn write(&mut self, data: &[u8]) -> io::Result<()> {
        self.write_stream.write_u32(data.len() as u32).await?;
        self.write_stream.write(data).await?;

        self.write_stream.flush().await?;

        Ok(())
    }

    #[allow(dead_code)]
    pub async fn write_chunks(&mut self, chunks: &[&[u8]]) -> io::Result<()> {
        let length = chunks.iter().map(|data| data.len()).sum::<usize>() as u32;

        self.write_stream.write_u32(length).await?;

        for data in chunks {
            self.write_stream.write(data).await?;
        }

        Ok(())
    }

    pub fn verify(&mut self, data: &[u8], tag: &[u8]) -> bool {
        self.mac.update(data);

        let verify_tag: [u8; 8] = self.mac.finalize_reset().into_bytes().as_slice()[0..8]
            .try_into()
            .unwrap();

        verify_tag == tag
    }

    pub fn sign(&mut self, data: &[u8]) -> [u8; 8] {
        self.mac.update(data);

        self.mac.finalize_reset().into_bytes().as_slice()[0..8]
            .try_into()
            .unwrap()
    }
}
