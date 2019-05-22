cargo build --release
TESTS=(
# Stalls with blank screen
tests/cpu/instr_timing.nes
# 1D 19 11 3D 39 31 5D 59 51 7D
# 79 71 9D 99 91 BD B9 B1 DD D9
# D1 FD F9 F1 1E 3E 5E 7E DE FE
# BC BE
# failed 04-dummy_reads_apu #2
tests/cpu/instr_misc.nes
# 4000 FF ERROR
# Landed at $1AB5 failed #2 to obey path
tests/cpu/exec_space_apu.nes
# Spins endlessly
tests/cpu/flag_concurrency.nes
# Stops at test 1
tests/cpu/all_instrs.nes
# No output
tests/cpu/interrupts.nes
# Doesn't sound right
tests/apu/triangle.nes
tests/apu/phase_reset.nes
tests/apu/dmc.nes
tests/apu/square.nes
# $02
tests/apu/05.len_timing_mode0.nes
# $02
tests/apu/06.len_timing_mode1.nes
# $03
tests/apu/07.irq_flag_timing.nes
# $04
tests/apu/08.irq_timing.nes
# $04
tests/apu/09.reset_timing.nes
# $05
tests/apu/11.len_reload_timing.nes
# no output
tests/apu/test.nes
# 09 clock skipped too soon relative to enabling BG
# 10-even_odd_timing #2 1/10
tests/ppu/vbl_nmi.nes
# 59916E5B
tests/ppu/oam_stress.nes
# Flickers
tests/ppu/palette.nes
# Freezes
tests/ppu/sprdma_and_dmc_dma_512.nes
tests/ppu/sprdma_and_dmc_dma.nes
# Can't read PASS
tests/ppu/tv.nes
# failed #3
tests/ppu/vbl_nmi_timing/6.nmi_disable.nes
# failed #5 - nes_fog passes
tests/ppu/vbl_nmi_timing/1.frame_basics.nes
# failed #4
tests/ppu/vbl_nmi_timing/5.nmi_suppression.nes
# failed #3
tests/ppu/vbl_nmi_timing/3.even_odd_frames.nes
# failed #3
tests/ppu/vbl_nmi_timing/7.nmi_timing.nes
# failed #8
tests/ppu/vbl_nmi_timing/2.vbl_timing.nes
# No raster effects
tests/ppu/ntsc_torture.nes
# decay should happen after 1 sec
# #3
tests/ppu/open_bus.nes
# disabling rendering didn't recalculate flag time
# 05-emulator #3 1/5
tests/ppu/sprite_overflow.nes
# failed #3 - nes_fog passes
tests/ppu/vbl_clear_time.nes
# garbled display
tests/ppu/nmi_sync_ntsc.nes
# upper-left corner
# 10-timing_order #2 1/10
tests/ppu/sprite_hit.nes
# flickers - nes_fog passes
tests/ppu/scanline.nes
# Passes but message doesn't disapper
tests/cpu/registers_after_reset.nes
# Passes but message doesn't disappear
tests/cpu/ram_after_reset.nes
)

trap ctrl_c INT

function ctrl_c() {
    echo "** Trapped CTRL-C...Exiting"
    exit
}

for test in ${TESTS[*]}; do
    echo $test
    target/release/rustynes $test
done

