use tokio::{
    io,
    io::{AsyncRead, AsyncReadExt, AsyncWrite, ReadHalf},
};

pub async fn read_frame<T: AsyncRead + AsyncWrite>(
    read_stream: &mut ReadHalf<T>,
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
