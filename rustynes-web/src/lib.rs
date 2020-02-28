use rustynes::{
    bus::Bus,
    common::Clocked,
    cpu::Cpu,
    mapper,
    ui::{Ui, UiSettings},
    NesErr,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn main() -> Result<(), JsValue> {
    // Ensure panics print to console
    set_panic_hook();

    // Use `web_sys`'s global `window` function to get a handle on the global
    // window object.
    let window = web_sys::window().expect("no global `window` exists");
    let document = window.document().expect("should have a document on window");
    let body = document.body().expect("document should have a body");

    let mut cpu = Cpu::init(Bus::new());
    // let mapper = mapper::load_rom(
    //     "/Users/caeledh/dev/rustynes/rustynes-web/roms/castlevania_iii_draculas_curse.nes",
    // )?;
    // cpu.bus.load_mapper(mapper);
    for _ in 0..20 {
        let val = document.create_element("div")?;
        val.set_inner_html(&format!("${:04X}", cpu.pc));
        cpu.clock();
        body.append_child(&val)?;
    }
    // let ui = Ui::new();
    // ui.run()?;
    // if let Err(e) = ui.run() {
    //     eprintln!("Error: {}", e);
    // }

    // let canvas = document.get_element_by_id("canvas").unwrap();
    // let canvas: web_sys::HtmlCanvasElement = canvas
    //     .dyn_into::<web_sys::HtmlCanvasElement>()
    //     .map_err(|_| ())
    //     .unwrap();
    // let context = canvas
    //     .get_context("2d")
    //     .unwrap()
    //     .unwrap()
    //     .dyn_into::<web_sys::CanvasRenderingContext2d>()
    //     .unwrap();

    // context.begin_path();

    // // Draw the outer circle.
    // context
    //     .arc(75.0, 75.0, 50.0, 0.0, f64::consts::PI * 2.0)
    //     .unwrap();

    // // Draw the mouth.
    // context.move_to(110.0, 75.0);
    // context.arc(75.0, 75.0, 35.0, 0.0, f64::consts::PI).unwrap();

    // // Draw the left eye.
    // context.move_to(65.0, 65.0);
    // context
    //     .arc(60.0, 65.0, 5.0, 0.0, f64::consts::PI * 2.0)
    //     .unwrap();

    // // Draw the right eye.
    // context.move_to(95.0, 65.0);
    // context
    //     .arc(90.0, 65.0, 5.0, 0.0, f64::consts::PI * 2.0)
    //     .unwrap();

    // context.stroke();

    Ok(())
}

pub fn set_panic_hook() {
    // When the `console_error_panic_hook` feature is enabled, we can call the
    // `set_panic_hook` function at least once during initialization, and then
    // we will get better error messages if our code ever panics.
    //
    // For more details see
    // https://github.com/rustwasm/console_error_panic_hook#readme
    #[cfg(feature = "console_error_panic_hook")]
    console_error_panic_hook::set_once();
}
