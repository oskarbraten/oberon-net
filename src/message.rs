#[derive(Debug, Clone)]
pub struct Message {
    pub data: Vec<u8>,
    pub reliable: bool,
}

impl Message {
    pub fn reliable(data: Vec<u8>) -> Self {
        Self {
            data,
            reliable: true,
        }
    }

    pub fn unreliable(data: Vec<u8>) -> Self {
        Self {
            data,
            reliable: false,
        }
    }
}
