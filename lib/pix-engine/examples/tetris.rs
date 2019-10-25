use pix_engine::{event::*, pixel::Pixel, *};

const BEVEL_SIZE: u32 = 2;
const BLOCK_SIZE: u32 = 24;
const TETRO_SIZE: u32 = 4 * BLOCK_SIZE;
const FIELD_WIDTH: u32 = 12;
const FIELD_HEIGHT: u32 = 18;
const FIELD_TOP: u32 = 96;
const FIELD_BOTTOM: u32 = FIELD_TOP + FIELD_HEIGHT * BLOCK_SIZE - BLOCK_SIZE;
const FIELD_LEFT: u32 = 144;
const FIELD_RIGHT: u32 = FIELD_LEFT + FIELD_WIDTH * BLOCK_SIZE;
const SCORE_LEFT: u32 = FIELD_RIGHT + 4 * BLOCK_SIZE;

struct App {
    width: u32,
    height: u32,
    level: u32,
    score: u32,
    lines: u32,
    paused: bool,
    tetrominos: Vec<Sprite>,
    field: Sprite,
    current_tetro: Option<Sprite>,
    current_rotation: f32,
    current_x: u32,
    current_y: u32,
}

impl App {
    fn new() -> Self {
        let width = BLOCK_SIZE * 33;
        let height = BLOCK_SIZE * 25;
        Self {
            width,
            height,
            level: 1,
            score: 0,
            lines: 0,
            paused: false,
            tetrominos: Vec::new(),
            field: Sprite::new(width, height),
            current_tetro: None,
            current_rotation: 0.0,
            current_x: 0,
            current_y: 0,
        }
    }

    fn create_block(p: Pixel, data: &mut StateData) -> Sprite {
        let block = Sprite::new(BLOCK_SIZE, BLOCK_SIZE);
        data.set_draw_target(block);
        App::draw_block(0, 0, p, data);
        data.take_draw_target().expect("valid draw target")
    }

    fn draw_block(x: u32, y: u32, p: Pixel, data: &mut StateData) {
        data.fill_rect(x * BLOCK_SIZE, y * BLOCK_SIZE, BLOCK_SIZE, BLOCK_SIZE, p);
        let highlight = Pixel([
            p[0].saturating_mul(2),
            p[1].saturating_mul(2),
            p[2].saturating_mul(2),
            p[3],
        ]);
        data.fill_rect(
            x * BLOCK_SIZE,
            y * BLOCK_SIZE,
            BEVEL_SIZE,
            BLOCK_SIZE,
            highlight,
        ); // Left bevel
        data.fill_rect(
            x * BLOCK_SIZE,
            y * BLOCK_SIZE,
            BLOCK_SIZE,
            BEVEL_SIZE,
            highlight,
        ); // Top bevel
        let shadow = Pixel([p[0] / 4, p[1] / 4, p[2] / 4, p[3]]);
        data.fill_rect(
            x * BLOCK_SIZE + BLOCK_SIZE - BEVEL_SIZE,
            y * BLOCK_SIZE,
            BEVEL_SIZE,
            BLOCK_SIZE,
            shadow,
        ); // Right bevel
        data.fill_rect(
            x * BLOCK_SIZE,
            y * BLOCK_SIZE + BLOCK_SIZE - BEVEL_SIZE,
            BLOCK_SIZE,
            BEVEL_SIZE,
            shadow,
        ); // Bottom bevel
    }

    // TODO Move all tetros to be matrices with 1s and 0s
    // Abstract the draw function to take a matrix, a color, and a rotation
    // Rotate the matrix, then re-draw the blocks
    // Can then use the matrices for collision as well?

    // #
    // # ####
    // #
    // #
    fn tetro_i(rotation: u32, data: &mut StateData) -> Sprite {
        let sprite = Sprite::new(TETRO_SIZE, TETRO_SIZE);
        data.set_draw_target(sprite);
        data.fill(pixel::TRANSPARENT);
        if rotation % 2 == 0 {
            for y in 0..4 {
                App::draw_block(0, y, pixel::CYAN, data);
            }
        } else {
            for x in 0..4 {
                App::draw_block(x, 1, pixel::CYAN, data);
            }
        }
        data.take_draw_target().expect("valid draw target")
    }
    // ##
    // ##
    fn tetro_o(data: &mut StateData) -> Sprite {
        let sprite = Sprite::new(TETRO_SIZE, TETRO_SIZE);
        data.set_draw_target(sprite);
        data.fill(pixel::TRANSPARENT);
        for x in 0..2 {
            for y in 0..2 {
                App::draw_block(x, y, pixel::YELLOW, data);
            }
        }
        data.take_draw_target().expect("valid draw target")
    }
    //  #   #         #
    // ###  ##  ###  ##
    //      #    #    #
    fn tetro_t(data: &mut StateData) -> Sprite {
        let sprite = Sprite::new(TETRO_SIZE, TETRO_SIZE);
        data.set_draw_target(sprite);
        data.fill(pixel::TRANSPARENT);
        App::draw_block(1, 0, pixel::MAGENTA, data);
        for x in 0..3 {
            App::draw_block(x, 1, pixel::MAGENTA, data);
        }
        data.take_draw_target().expect("valid draw target")
    }
    //  #  #    ##
    //  #  ###  #   ###
    // ##       #     #
    fn tetro_j(data: &mut StateData) -> Sprite {
        let sprite = Sprite::new(TETRO_SIZE, TETRO_SIZE);
        data.set_draw_target(sprite);
        data.fill(pixel::TRANSPARENT);
        App::draw_block(0, 2, pixel::BLUE, data);
        for y in 0..3 {
            App::draw_block(1, y, pixel::BLUE, data);
        }
        data.take_draw_target().expect("valid draw target")
    }
    // #        ##    #
    // #   ###   #  ###
    // ##  #     #
    fn tetro_l(data: &mut StateData) -> Sprite {
        let sprite = Sprite::new(TETRO_SIZE, TETRO_SIZE);
        data.set_draw_target(sprite);
        data.fill(pixel::TRANSPARENT);
        App::draw_block(1, 2, pixel::ORANGE, data);
        for y in 0..3 {
            App::draw_block(0, y, pixel::ORANGE, data);
        }
        data.take_draw_target().expect("valid draw target")
    }
    //      #
    //  ##  ##
    // ##    #
    fn tetro_s(data: &mut StateData) -> Sprite {
        let sprite = Sprite::new(TETRO_SIZE, TETRO_SIZE);
        data.set_draw_target(sprite);
        data.fill(pixel::TRANSPARENT);
        App::draw_block(1, 0, pixel::GREEN, data);
        App::draw_block(2, 0, pixel::GREEN, data);
        App::draw_block(0, 1, pixel::GREEN, data);
        App::draw_block(1, 1, pixel::GREEN, data);
        data.take_draw_target().expect("valid draw target")
    }
    //       #
    // ##   ##
    //  ##  #
    fn tetro_z(data: &mut StateData) -> Sprite {
        let sprite = Sprite::new(TETRO_SIZE, TETRO_SIZE);
        data.set_draw_target(sprite);
        data.fill(pixel::TRANSPARENT);
        App::draw_block(0, 0, pixel::RED, data);
        App::draw_block(1, 0, pixel::RED, data);
        App::draw_block(1, 1, pixel::RED, data);
        App::draw_block(2, 1, pixel::RED, data);
        data.take_draw_target().expect("valid draw target")
    }

    // (2, 2) -> 10
    // (2, 2) ->
    // 0   ..94, 95
    // 96  ..190, 191
    // 9024..9118, 9119
    // 9120..9214, 9215
    fn rotated(x: u32, y: u32, r: u32) -> u32 {
        match r % 4 {
            0 => y * TETRO_SIZE + x, // 0 degrees
            1 => TETRO_SIZE * TETRO_SIZE - TETRO_SIZE + y - (x * TETRO_SIZE), // 90 degrees
            2 => TETRO_SIZE * TETRO_SIZE - (y * TETRO_SIZE) - x, // 180 degrees
            3 => TETRO_SIZE - 1 - y + (x * TETRO_SIZE), // 270 degrees
            _ => panic!("impossible"),
        }
    }

    // fn valid_move(&mut self, tetro_id: usize, rotation: u32, x: u32, y: u32) -> bool {
    //     for tx in 0..TETRO_SIZE {
    //         for ty in 0..TETRO_SIZE {
    //             // Index into piece
    //             let ti = App::rotated(tx, ty, rotation);

    //             // Index into field
    //             let fi = (y + ty) * BLOCK_SIZE * FIELD_WIDTH + (x + tx);

    //             if x + tx < BLOCK_SIZE * FIELD_WIDTH {
    //                 if y + ty < BLOCK_SIZE * FIELD_HEIGHT {
    //                     if self.tetrominos[tetro_id].as_bytes()[ti as usize + 3] != 0
    //                         && data.get_draw_target().as_bytes()[fi as usize + 3] != 0
    //                     {
    //                         return false;
    //                     }
    //                 }
    //             }
    //         }
    //     }
    //     true
    // }

    fn create_field(&mut self, data: &mut StateData) -> Sprite {
        // Draw background field
        let block = App::create_block(pixel::GRAY, data);
        let field = Sprite::new(self.width, self.height);

        data.set_draw_target(field);
        data.fill(pixel::TRANSPARENT);
        // Left wall
        for x in (0..FIELD_LEFT).step_by(BLOCK_SIZE as usize) {
            for y in (0..data.screen_height()).step_by(BLOCK_SIZE as usize) {
                data.draw_sprite(x, y, &block);
            }
        }
        // Right
        for x in (FIELD_RIGHT..data.screen_width()).step_by(BLOCK_SIZE as usize) {
            for y in (0..data.screen_height()).step_by(BLOCK_SIZE as usize) {
                data.draw_sprite(x, y, &block);
            }
        }

        // Bottom
        for x in (FIELD_LEFT..FIELD_RIGHT).step_by(BLOCK_SIZE as usize) {
            for y in (FIELD_BOTTOM..data.screen_height()).step_by(BLOCK_SIZE as usize) {
                data.draw_sprite(x, y, &block);
            }
        }

        // Draw scoreboard information
        data.fill_rect(504, 48, 168, 408, pixel::BLACK);
        data.draw_string(534, 100, "SCORE", pixel::WHITE);
        data.draw_string(534, 132, &format!("{}", self.score), pixel::WHITE);
        data.draw_string(534, 200, "LEVEL", pixel::WHITE);
        data.draw_string(534, 232, &format!("{}", self.level), pixel::WHITE);
        data.draw_string(534, 300, "LINES", pixel::WHITE);
        data.draw_string(534, 332, &format!("{}", self.lines), pixel::WHITE);
        data.take_draw_target().expect("valid draw target")
    }
}

impl State for App {
    fn on_start(&mut self, data: &mut StateData) -> PixEngineResult<()> {
        self.tetrominos.push(App::tetro_i(0, data));
        self.tetrominos.push(App::tetro_o(data));
        self.tetrominos.push(App::tetro_t(data));
        self.tetrominos.push(App::tetro_j(data));
        self.tetrominos.push(App::tetro_l(data));
        self.tetrominos.push(App::tetro_s(data));
        self.tetrominos.push(App::tetro_z(data));
        self.field = self.create_field(data);
        self.current_y = FIELD_TOP;
        self.current_x = FIELD_LEFT + (FIELD_RIGHT - FIELD_LEFT) / 2;
        self.current_tetro = Some(self.tetrominos[rand::random::<usize>() % 7].clone());
        data.clear();
        data.set_draw_scale(3);
        data.draw_sprite(0, 0, &self.field);
        data.set_draw_scale(1);
        Ok(())
    }
    fn on_update(&mut self, elapsed: f32, data: &mut StateData) -> PixEngineResult<()> {
        data.fill_rect(
            FIELD_LEFT,
            0,
            BLOCK_SIZE * FIELD_WIDTH,
            BLOCK_SIZE * FIELD_HEIGHT,
            pixel::BLACK,
        );

        if data.get_key(Key::Escape).pressed {
            self.paused = !self.paused;
        }
        if data.get_key(Key::R).pressed {
            // TODO restart game
        }
        if self.paused {
            return Ok(());
        }

        if data.get_key(Key::Z).held {
            self.current_rotation -= 2.0 * elapsed;
        }
        if data.get_key(Key::X).held {
            self.current_rotation += 2.0 * elapsed;
        }
        if data.get_key(Key::Right).held && self.current_x < (FIELD_RIGHT - BLOCK_SIZE) {
            self.current_x += (5.0 * elapsed).ceil() as u32;
        } else if data.get_key(Key::Left).held && self.current_x > (FIELD_LEFT + BLOCK_SIZE) {
            self.current_x -= (5.0 * elapsed).ceil() as u32;
        }
        if data.get_key(Key::Down).held
            && self.current_y < (FIELD_BOTTOM - self.current_tetro.as_ref().unwrap().height())
        {
            self.current_y += (5.0 * elapsed).ceil() as u32;
        }

        data.set_alpha_mode(AlphaMode::Mask);
        data.draw_sprite(
            self.current_x,
            self.current_y,
            self.current_tetro.as_ref().unwrap(),
        );
        data.set_alpha_mode(AlphaMode::Normal);
        Ok(())
    }
}

pub fn main() {
    let app = App::new();
    let w = app.width;
    let h = app.height;
    let mut engine = PixEngine::new("Tetris", app, w, h, false).unwrap();
    if let Err(e) = engine.run() {
        eprintln!("Encountered a PixEngineErr: {}", e.to_string());
    }
}
