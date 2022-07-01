# Config JSON

Configuration for `TetaNES` is stored in `config.json` with defaults that can be
customized in the `TetaNES` config menu.

Outlined below is the explanation of the more complicated structure of the
configuration.

## Bindings

### Keyboard Mappings

A `keys` array with the following values:

- `player`: The player this keybinding should apply to (`One`, `Two`, `Three`,
  or `Four`).
- `key`: A string that maps to a `pix_engine::prelude::Key` variant.
- `keymod`: A number that maps to a `pix_engine::prelude::KeyMod` constant:
  - `NONE`: `0`
  - `SHIFT`: `1`,
  - `CTRL`: `64`,
  - `ALT`: `256`,
  - `GUI`: `1024`,
- `action`: An object that maps to an `nes::Action` variant. e.g.
  `{ "Joypad": "Left" } }`

### Mouse Mappings

A `mouse` array wit hhe following values:

- `player`: The player this button should apply to (`One`, `Two`, `Three`, or
  `Four`).
- `button`: A string that maps to a `pix_engine::prelud::Mouse` variant.
- `action`: An object that maps to an `Nes::Action` variant. e.g.
   `{ "Zapper": [0, 0] }`

### Controller Button Mappings

A `buttons` array with the following values:

- `player`: The player this button should apply to (`One`, `Two`, `Three`, or
  `Four`).
- `button`: A string that maps to a `pix_engine::prelude::ControllerButton`
  variant.
- `action`: An object that maps to an `nes::Action` variant. e.g.
  `{ "Nes": ToggleMenu" } }`

### Controller Axis Mappings

A `axes` array with the following values:

- `player`: The player this button should apply to (`One`, `Two`, `Three`, or
  `Four`).
- `axis`: A string that maps to a `pix_engine::prelude::Axis` variant.
- `direction`: `None`, `Positive`, or `Negative` to indicate axis direction.
- `action`: An object that maps to an `nes::Action` variant. e.g.
  `{ "ZeroAxis": ["Left", "Right"] } }`
