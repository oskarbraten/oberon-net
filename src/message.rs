#[derive(Debug, Clone)]
pub struct Message {
    pub data: Vec<u8>,
    pub reliable: bool,
}
