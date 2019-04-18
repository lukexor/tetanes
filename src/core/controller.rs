// const (
//     button_a = iota
//     button_b
//     button_select
//     button_start
//     button_up
//     button_down
//     button_left
//     button_right
// )

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
