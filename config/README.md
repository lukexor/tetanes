# Config JSON

Configuration for `TetaNES` is stored in `~/.config/tetanes/config.json` with
defaults that can be customized in the `TetaNES` config menu.

## Bindings

### Keyboard Mappings

A `keys` array with the following values:

- `controller`: The controller this keybinding should apply to (`One`, `Two`,
  `Three`, or `Four`).
- `key`: A string that maps to a `pix_engine::prelude::Key` variant.
- `keymod`: A number that maps to a `pix_engine::prelude::KeyMod` constant:
  - `NONE`: `-1`
  - `SHIFT`: `0`,
  - `CTRL`: `63`,
  - `ALT`: `255`,
  - `GUI`: `1023`,
- `action`: An object that maps to an `nes::Action` variant. e.g.
  `{ "Joypad": "Left" } }`

### Mouse Mappings

A `mouse` array with the following values:

- `controller`: The controller this button should apply to (`One`, `Two`,
  `Three`, or `Four`).
- `button`: A string that maps to a `pix_engine::prelud::Mouse` variant.
- `action`: An object that maps to an `Nes::Action` variant. e.g.
  `{ "Zapper": [0, 0] }`

### Controller Button Mappings

A `buttons` array with the following values:

- `controller`: The controller this button should apply to (`One`, `Two`,
  `Three`, or `Four`).
- `button`: A string that maps to a `pix_engine::prelude::ControllerButton`
  variant.
- `action`: An object that maps to an `nes::Action` variant. e.g.
  `{ "Nes": ToggleMenu" } }`

### Controller Axis Mappings

A `axes` array with the following values:

- `controller`: The controller this button should apply to (`One`, `Two`,
  `Three`, or `Four`).
- `axis`: A string that maps to a `pix_engine::prelude::Axis` variant.
- `direction`: `None`, `Positive`, or `Negative` to indicate axis direction.
- `action`: An object that maps to an `nes::Action` variant. e.g.
  `{ "Feature": "SaveState" } }`
