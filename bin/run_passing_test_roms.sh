# Some tests rely on deterministic RAM state
cargo build --features no-randomize-ram
TESTS=(
## CPU ============================================================================================
tests/cpu/branch_timing/1.Branch_Basics.nes
tests/cpu/branch_timing/2.Backward_Branch.nes
tests/cpu/branch_timing/3.Forward_Branch.nes
tests/cpu/cpu_timing_test.nes
tests/cpu/flag_concurrency.nes
tests/cpu/dummy_reads.nes
tests/cpu/dummy_writes_oam.nes
tests/cpu/dummy_writes_ppumem.nes
tests/cpu/exec_space_apu.nes
tests/cpu/exec_space_ppuio.nes
tests/cpu/instr/01-implied.nes
tests/cpu/instr/02-immediate.nes
tests/cpu/instr/03-zero_page.nes
tests/cpu/instr/04-zp_xy.nes
tests/cpu/instr/05-absolute.nes
tests/cpu/instr/06-abs_xy.nes
tests/cpu/instr/07-ind_x.nes
tests/cpu/instr/08-ind_y.nes
tests/cpu/instr/09-branches.nes
tests/cpu/instr/10-stack.nes
tests/cpu/instr/11-jmp_jsr.nes
tests/cpu/instr/12-rts.nes
tests/cpu/instr/13-rti.nes
tests/cpu/instr/14-brk.nes
tests/cpu/instr/15-special.nes
tests/cpu/instr_misc.nes
tests/cpu/instr_timing.nes
tests/cpu/interrupts/1-cli_latency.nes
tests/cpu/interrupts/2-nmi_and_brk.nes
tests/cpu/interrupts/3-nmi_and_irq.nes
tests/cpu/interrupts/5-branch_delays_irq.nes
tests/cpu/nestest.nes
tests/cpu/overclock.nes
tests/cpu/ram_after_reset.nes
tests/cpu/registers_after_reset.nes

## APU ============================================================================================
# These tests have a white garbled background with randomized RAM
tests/apu/01.len_ctr.nes
tests/apu/02.len_table.nes
tests/apu/03.irq_flag.nes
tests/apu/04.clock_jitter.nes
tests/apu/08.irq_timing.nes

tests/apu/dmc/buffer_retained.nes
tests/apu/dmc/latency.nes
tests/apu/dmc/status.nes
tests/apu/dmc/status_irq.nes
tests/apu/test_1.nes
tests/apu/test_2.nes
tests/apu/test_5.nes
tests/apu/test_6.nes

# Audio tests - Skip for now
# tests/apu/apu_env.nes
# tests/apu/dmc/dmc.nes
# tests/apu/dmc/dmc_pitch.nes
# tests/apu/lin_ctr.nes
# tests/apu/noise.nes
# tests/apu/noise_pitch.nes
# tests/apu/phase_reset.nes
# tests/apu/square.nes
# tests/apu/square_pitch.nes
# tests/apu/sweep_cutoff.nes
# tests/apu/sweep_sub.nes
# tests/apu/triangle.nes
# tests/apu/triangle_pitch.nes
# tests/apu/volumes.nes

## PPU ============================================================================================
tests/ppu/oam_read.nes
tests/ppu/oam_stress.nes
tests/ppu/open_bus.nes
tests/ppu/palette.nes
tests/ppu/palette_ram.nes
tests/ppu/read_buffer.nes
tests/ppu/scanline.nes
tests/ppu/sprite_ram.nes
tests/ppu/sprite_hit/01-basics.nes
tests/ppu/sprite_hit/02-alignment.nes
tests/ppu/sprite_hit/03-corners.nes
tests/ppu/sprite_hit/04-flip.nes
tests/ppu/sprite_hit/05-left_clip.nes
tests/ppu/sprite_hit/06-right_edge.nes
tests/ppu/sprite_hit/07-screen_bottom.nes
tests/ppu/sprite_hit/08-double_height.nes
tests/ppu/sprite_hit/10-timing_order.nes
tests/ppu/sprite_overflow/1.Basics.nes
tests/ppu/sprite_overflow/2.Details.nes
tests/ppu/sprite_overflow/5.Emulator.nes
tests/ppu/vbl_clear_time.nes
tests/ppu/vbl_nmi/01-vbl_basics.nes
tests/ppu/vbl_nmi/02-vbl_set_time.nes
tests/ppu/vbl_nmi/03-vbl_clear_time.nes
tests/ppu/vbl_nmi/04-nmi_control.nes
tests/ppu/vbl_nmi/05-nmi_timing.nes
tests/ppu/vbl_nmi/06-suppression.nes
tests/ppu/vbl_nmi/08-nmi_off_timing.nes
tests/ppu/vbl_nmi/09-even_odd_frames.nes
tests/ppu/vbl_nmi_timing/1.frame_basics.nes
tests/ppu/vbl_nmi_timing/2.vbl_timing.nes
tests/ppu/vbl_nmi_timing/3.even_odd_frames.nes
tests/ppu/vbl_nmi_timing/4.vbl_clear_timing.nes
tests/ppu/vbl_nmi_timing/5.nmi_suppression.nes
tests/ppu/vbl_nmi_timing/6.nmi_disable.nes
tests/ppu/vbl_nmi_timing/7.nmi_timing.nes
tests/ppu/vram_access.nes

# Video - Skip for now
# tests/ppu/240pee.nes
# tests/ppu/color.nes
# tests/ppu/ntsc_torture.nes
# tests/ppu/tv.nes

## MAPPERS ========================================================================================
tests/mapper/mmc3/1.Clocking.nes
tests/mapper/mmc3/2.Details.nes
tests/mapper/mmc3/3.A12_clocking.nes
tests/mapper/mmc3/4.Scanline_timing.nes
tests/mapper/mmc3/6.MMC3_rev_B.nes
tests/mapper/mmc3/mmc3bigchrram.nes

)

trap ctrl_c INT

function ctrl_c() {
    echo "** Trapped CTRL-C...Exiting"
    exit
}

for test in ${TESTS[*]}; do
    echo $test
    target/debug/rustynes --speed 4 $test
done

