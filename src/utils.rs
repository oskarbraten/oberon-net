use aes::Aes128;
use cmac::{Cmac, Mac, NewMac};
use std::convert::TryInto;
use tokio::{io, io::AsyncReadExt, net::tcp::OwnedReadHalf};

pub async fn read_frame(
    read_stream: &mut OwnedReadHalf,
    max_frame_size: u32,
) -> io::Result<Vec<u8>> {
    let frame_size = {
        let mut bytes = [0; 4];
        read_stream.read_exact(&mut bytes).await?;

        u32::from_be_bytes(bytes)
    };
    if frame_size > max_frame_size {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Max frame size exceeded.",
        ));
    }

    let mut buffer = vec![0u8; frame_size as usize];
    read_stream.read_exact(&mut buffer).await?;

    Ok(buffer)
}

pub fn verify(key: &[u8; 16], data: &[u8], tag: &[u8]) -> bool {
    let mut mac = Cmac::<Aes128>::new_varkey(key).unwrap();
    mac.update(data);

    let verify_tag: [u8; 8] = mac.finalize().into_bytes().as_slice()[0..8]
        .try_into()
        .unwrap();

    verify_tag == tag
}

pub fn sign(key: &[u8; 16], data: &[u8]) -> [u8; 8] {
    let mut mac = Cmac::<Aes128>::new_varkey(key).unwrap();
    mac.update(data);

    mac.finalize().into_bytes().as_slice()[0..8]
        .try_into()
        .unwrap()
}
