// var pulse_table [31]float32
// var tnd_table [203]float32

// var Palette [64]color.RGBA

// // Mirroring Modes
// const (
//     mirror_horizontal = 0
//     mirror_vertical   = 1
//     mirror_single0    = 2
//     mirror_single1    = 3
//     mirror_four       = 4
// )

// var mirror_lookup = [...][4]u16{
//     {0, 0, 1, 1},
//     {0, 1, 0, 1},
//     {0, 0, 0, 0},
//     {1, 1, 1, 1},
//     {0, 1, 2, 3},
// }

// func init() {
//     for i := 0; i < 31; i++ {
//         pulse_table[i] = 95.52 / (8128.0/float32(i) + 100)
//     }
//     for i := 0; i < 203; i++ {
//         tnd_table[i] = 163.67 / (24329.0/float32(i) + 100)
//     }

//     colors := []u32{
//         0x666666, 0x002A88, 0x1412A7, 0x3B00A4, 0x5C007E, 0x6E0040, 0x6C0600, 0x561D00,
//         0x333500, 0x0B4800, 0x005200, 0x004F08, 0x00404D, 0x000000, 0x000000, 0x000000,
//         0x_aDADAD, 0x155FD9, 0x4240FF, 0x7527FE, 0x_a01ACC, 0x_b71E7B, 0x_b53120, 0x994E00,
//         0x6B6D00, 0x388700, 0x0C9300, 0x008F32, 0x007C8D, 0x000000, 0x000000, 0x000000,
//         0x_fFFEFF, 0x64B0FF, 0x9290FF, 0x_c676FF, 0x_f36AFF, 0x_fE6ECC, 0x_fE8170, 0x_eA9E22,
//         0x_bCBE00, 0x88D800, 0x5CE430, 0x45E082, 0x48CDDE, 0x4F4F4F, 0x000000, 0x000000,
//         0x_fFFEFF, 0x_c0DFFF, 0x_d3D2FF, 0x_e8C8FF, 0x_fBC2FF, 0x_fEC4EA, 0x_fECCC5, 0x_f7D8A5,
//         0x_e4E594, 0x_cFEF96, 0x_bDF4AB, 0x_b3F3CC, 0x_b5EBF2, 0x_b8B8B8, 0x000000, 0x000000,
//     }
//     for i, c := range colors {
//         r := u8(c >> 16)
//         g := u8(c >> 8)
//         b := u8(c)
//         Palette[i] = color.RGBA{r, g, b, 0x_fF}
//     }
// }
