use super::*;
use std::path::PathBuf;

pub struct Director {
    pub window: glfw::Window,
    pub audio: Audio,
    // pub view: Box<View>,
    pub view: MenuView,
    // pub menu_view: MenuView,
    pub timestamp: f64,
}

impl Director {
    pub fn new(window: glfw::Window, audio: Audio, roms: Vec<PathBuf>) -> Self {
        // let menu_view2 = MenuView::new(roms.clone());
        let menu_view = MenuView::new(roms);
        Director {
            window,
            audio,
            // view: Box::new(menu_view2),
            view: menu_view,
            // menu_view,
            timestamp: 0.0,
        }
    }

    //     pub fn set_game_view(&mut self, view: GameView) {
    //         self.view.reset();
    //         self.view = Box::new(GameView);
    //         self.view.setup();
    //     }

    //     pub fn set_menu_view(&mut self, view: MenuView) {
    //         self.view.reset();
    //         self.view = Box::new(MenuView);
    //         self.view.setup();
    //     }

    pub fn setup_view(&mut self) {
        self.window.set_char_polling(false);
        unsafe {
            gl::ClearColor(0.333, 0.333, 0.333, 1.0);
        }
        self.window.set_title("Select Game");
        self.window.set_char_polling(true);

        // self.view.reset(&mut self.window);
        // self.view.setup();
    }
}
