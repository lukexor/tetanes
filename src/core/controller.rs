#[derive(Default, Debug)]
pub struct Controller {
    pub buttons: [bool; 8],
    pub index: u8,
    pub strobe: u8,
}

impl Controller {
    pub fn new() -> Self {
        Controller {
            ..Default::default()
        }
    }
}
