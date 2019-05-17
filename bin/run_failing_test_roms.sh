cargo build --release
target/release/rustynes tests/cpu/instr_timing.nes
# 1D 19 11 3D 39 31 5D 59 51 7D
# 79 71 9D 99 91 BD B9 B1 DD D9
# D1 FD F9 F1 1E 3E 5E 7E DE FE
# BC BE
# failed 04-dummy_reads_apu #2
target/release/rustynes tests/cpu/instr_misc.nes
# Passes but message doesn't disapper
target/release/rustynes tests/cpu/registers_after_reset.nes
# Passes but message doesn't disappear
target/release/rustynes tests/cpu/ram_after_reset.nes
# 4000 FF ERROR
# Landed at $1AB5 failed #2 to obey path
target/release/rustynes tests/cpu/exec_space_apu.nes
# Spins endlessly
target/release/rustynes tests/cpu/flag_concurrency.nes
# Stops at test 1
target/release/rustynes tests/cpu/all_instrs.nes
# No output
target/release/rustynes tests/cpu/interrupts.nes
# $04
target/release/rustynes tests/apu/08.irq_timing.nes
# $03
target/release/rustynes tests/apu/04.clock_jitter.nes
# Doesn't sound right
target/release/rustynes tests/apu/triangle.nes
target/release/rustynes tests/apu/phase_reset.nes
target/release/rustynes tests/apu/dmc.nes
target/release/rustynes tests/apu/square.nes
# $03
target/release/rustynes tests/apu/06.len_timing_mode1.nes
# $03
target/release/rustynes tests/apu/10.len_halt_timing.nes
# $02
target/release/rustynes tests/apu/03.irq_flag.nes
# F8 FF 1E 02
target/release/rustynes tests/apu/02.len_table.nes
# $03
target/release/rustynes tests/apu/01.len_ctr.nes
# $02
target/release/rustynes tests/apu/09.reset_timing.nes
# $03
target/release/rustynes tests/apu/05.len_timing_mode0.nes
# $02
target/release/rustynes tests/apu/11.len_reload_timing.nes
# $03
target/release/rustynes tests/apu/07.irq_flag_timing.nes
# no output
target/release/rustynes tests/apu/test.nes
# 09 clock skipped too soon relative to enabling BG
# 10-even_odd_timing #2 1/10
target/release/rustynes tests/ppu/vbl_nmi.nes
# 59916E5B
target/release/rustynes tests/ppu/oam_stress.nes
# Flickers
target/release/rustynes tests/ppu/palette.nes
# Freezes
target/release/rustynes tests/ppu/sprdma_and_dmc_dma_512.nes
target/release/rustynes tests/ppu/sprdma_and_dmc_dma.nes
# Can't read PASS
target/release/rustynes tests/ppu/tv.nes
# failed #3
target/release/rustynes tests/ppu/vbl_nmi_timing/6.nmi_disable.nes
# failed #5 - nes_fog passes
target/release/rustynes tests/ppu/vbl_nmi_timing/1.frame_basics.nes
# failed #4
target/release/rustynes tests/ppu/vbl_nmi_timing/5.nmi_suppression.nes
# failed #3
target/release/rustynes tests/ppu/vbl_nmi_timing/3.even_odd_frames.nes
# failed #3
target/release/rustynes tests/ppu/vbl_nmi_timing/7.nmi_timing.nes
# failed #8
target/release/rustynes tests/ppu/vbl_nmi_timing/2.vbl_timing.nes
# No raster effects
target/release/rustynes tests/ppu/ntsc_torture.nes
# decay should happen after 1 sec
# #3
target/release/rustynes tests/ppu/open_bus.nes
# disabling rendering didn't recalculate flag time
# 05-emulator #3 1/5
target/release/rustynes tests/ppu/sprite_overflow.nes
# failed #3 - nes_fog passes
target/release/rustynes tests/ppu/vbl_clear_time.nes
# garbled display
target/release/rustynes tests/ppu/nmi_sync_ntsc.nes
# upper-left corner
# 10-timing_order #2 1/10
target/release/rustynes tests/ppu/sprite_hit.nes
# flickers - nes_fog passes
target/release/rustynes tests/ppu/scanline.nes
