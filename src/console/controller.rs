// #[derive(Default, Debug)]
// pub struct Controller {
//     pub buttons: [bool; 8],
//     pub index: u8,
//     pub strobe: u8,
// }

// impl Controller {
//     pub fn new() -> Self {
//         Self {
//             ..Default::default()
//         }
//     }

//     pub fn read(&mut self) -> u8 {
//         let val = if self.index < 8 && self.buttons[self.index as usize] {
//             1
//         } else {
//             0
//         };
//         self.index += 1;
//         if self.strobe & 1 == 1 {
//             self.index = 0;
//         }
//         val
//     }

//     pub fn write(&mut self, val: u8) {
//         self.strobe = val;
//         if self.strobe & 1 == 1 {
//             self.index = 0;
//         }
//     }
// }
