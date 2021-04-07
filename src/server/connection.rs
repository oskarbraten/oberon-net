use aes::Aes128;
use cmac::{Cmac, Mac, NewMac};
use rand::RngCore;
use std::{convert::TryInto, net::SocketAddr};
use tokio::{io, io::AsyncWriteExt, net::tcp::OwnedWriteHalf};

#[derive(Debug)]
pub struct Connection {
    pub key: [u8; 16],
    pub write_stream: OwnedWriteHalf,
    pub udp_address: Option<SocketAddr>,
}

impl Connection {
    pub async fn new(id: u32, mut write_stream: OwnedWriteHalf) -> io::Result<Self> {
        let mut key = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut key);

        // Initiate handshake (1):
        write_stream.write_u32(4 + key.len() as u32).await?; // id (u32) size + key length
        write_stream.write_u32(id as u32).await?;
        write_stream.write(&key).await?;

        Ok(Self {
            key,
            write_stream,
            udp_address: None,
        })
    }

    pub fn verify(&self, data: &[u8], tag: &[u8]) -> bool {
        let mut mac = Cmac::<Aes128>::new_varkey(&self.key).unwrap();
        mac.update(data);

        mac.verify(tag).is_ok()
    }

    pub fn sign(&self, data: &[u8]) -> [u8; 8] {
        let mut mac = Cmac::<Aes128>::new_varkey(&self.key).unwrap();
        mac.update(data);

        mac.finalize().into_bytes().as_slice()[0..8]
            .try_into()
            .unwrap()
    }

    pub async fn write(&mut self, data: &[u8]) -> io::Result<()> {
        self.write_stream.write_u32(data.len() as u32).await?;
        self.write_stream.write(data).await?;

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
}
