use aes::Aes128;
use anyhow::Result;
use cmac::{Cmac, Mac, NewMac};
use rand::RngCore;
use std::{convert::TryInto, net::SocketAddr};
use tokio::{
    io,
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadHalf, WriteHalf},
    net::UdpSocket,
    sync::Mutex,
    time::{sleep, Duration},
};

#[derive(Debug)]
pub struct Connection<T: AsyncRead + AsyncWrite> {
    pub key: [u8; 16],
    pub sign_mac: std::sync::Mutex<Cmac<Aes128>>,
    pub verify_mac: std::sync::Mutex<Cmac<Aes128>>,
    pub write_stream: Mutex<WriteHalf<T>>,
    pub address: Mutex<Option<SocketAddr>>,
}

impl<T> Connection<T>
where
    T: AsyncRead + AsyncWrite,
{
    pub async fn connect(
        socket: &UdpSocket,
        read_stream: &mut ReadHalf<T>,
        mut write_stream: WriteHalf<T>,
    ) -> Result<(u32, Self)> {
        let data = Self::read(read_stream, 2500).await?;
        let id = u32::from_be_bytes(data[0..4].try_into()?);
        let key: [u8; 16] = data[4..20].try_into()?;

        let mut sign_mac =
            Cmac::<Aes128>::new_varkey(&key).map_err(|err| anyhow::anyhow!("{}", err))?;
        let verify_mac =
            Cmac::<Aes128>::new_varkey(&key).map_err(|err| anyhow::anyhow!("{}", err))?;

        let tag: [u8; 8] = {
            sign_mac.update(b"ACK");

            sign_mac.finalize_reset().into_bytes().as_slice()[0..8]
                .try_into()
                .unwrap()
        };

        let mut ack = tag.to_vec(); // Add tag.
        ack.extend(&data[0..4]); // Add id (using raw received bytes).
                                 // ack.push(1); // Add mode (1 = ack).
        ack.extend(b"ACK"); // Add data.

        // Handshake - Send unreliable ACK (2):
        socket.send(&ack).await?;
        loop {
            tokio::select! {
                result = Self::read(read_stream, 80) => {
                    let data = result?;
                    if data == b"ACK" {
                        break;
                    }
                },
                _ = sleep(Duration::from_millis(128)) => {
                    // Retry. Packet was probably dropped or the latency is high.
                    socket.send(&ack).await?;
                }
            }
        }

        // Handshake - Send final reliable ACK (3)
        write_stream.write_u32(b"ACK".len() as u32).await?;
        write_stream.write(b"ACK").await?;

        Ok((
            id,
            Self {
                key,
                sign_mac: std::sync::Mutex::new(sign_mac),
                verify_mac: std::sync::Mutex::new(verify_mac),
                write_stream: Mutex::new(write_stream),
                address: Mutex::new(None),
            },
        ))
    }

    pub async fn accept(id: u32, mut write_stream: WriteHalf<T>) -> Result<Self> {
        let mut key = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut key);

        let sign_mac =
            Cmac::<Aes128>::new_varkey(&key).map_err(|err| anyhow::anyhow!("{}", err))?;
        let verify_mac =
            Cmac::<Aes128>::new_varkey(&key).map_err(|err| anyhow::anyhow!("{}", err))?;

        // Handshake - Initiate (1):
        write_stream.write_u32(4 + key.len() as u32).await?; // Connection id (u32) size + Key size
        write_stream.write_u32(id as u32).await?; // Connection id.
        write_stream.write(&key).await?; // Key.

        Ok(Self {
            key,
            sign_mac: std::sync::Mutex::new(sign_mac),
            verify_mac: std::sync::Mutex::new(verify_mac),
            write_stream: Mutex::new(write_stream),
            address: Mutex::new(None),
        })
    }

    pub async fn write(&self, data: &[u8]) -> io::Result<()> {
        let mut write_stream = self.write_stream.lock().await;
        write_stream.write_u32(data.len() as u32).await?;
        write_stream.write(data).await?;

        write_stream.flush().await?;

        Ok(())
    }

    pub async fn read(read_stream: &mut ReadHalf<T>, max_size: u32) -> io::Result<Vec<u8>> {
        let frame_size = {
            let mut bytes = [0; 4];
            read_stream.read_exact(&mut bytes).await?;

            u32::from_be_bytes(bytes)
        };
        if frame_size > max_size {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Max frame size exceeded.",
            ));
        }

        let mut buffer = vec![];
        read_stream
            .take(frame_size as u64)
            .read_to_end(&mut buffer)
            .await?;

        Ok(buffer)
    }

    pub fn verify(&self, data: &[u8], tag: &[u8]) -> bool {
        let mut mac = self.verify_mac.lock().unwrap();

        mac.update(data);

        let verify_tag: [u8; 8] = mac.finalize_reset().into_bytes().as_slice()[0..8]
            .try_into()
            .unwrap();

        verify_tag == tag
    }

    pub fn sign(&self, data: &[u8]) -> [u8; 8] {
        let mut mac = self.sign_mac.lock().unwrap();

        mac.update(data);

        mac.finalize_reset().into_bytes().as_slice()[0..8]
            .try_into()
            .unwrap()
    }
}