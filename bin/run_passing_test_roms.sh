# Some tests rely on deterministic RAM state
cargo build --release
TESTS=(
## CPU ============================================================================================
test_roms/cpu/branch_timing/1.Branch_Basics.nes
test_roms/cpu/branch_timing/2.Backward_Branch.nes
test_roms/cpu/branch_timing/3.Forward_Branch.nes
test_roms/cpu/cpu_timing_test.nes
test_roms/cpu/flag_concurrency.nes
test_roms/cpu/dummy_reads.nes
test_roms/cpu/dummy_writes_oam.nes
test_roms/cpu/dummy_writes_ppumem.nes
test_roms/cpu/exec_space_apu.nes
test_roms/cpu/exec_space_ppuio.nes
test_roms/cpu/instr/01-implied.nes
test_roms/cpu/instr/02-immediate.nes
test_roms/cpu/instr/03-zero_page.nes
test_roms/cpu/instr/04-zp_xy.nes
test_roms/cpu/instr/05-absolute.nes
test_roms/cpu/instr/06-abs_xy.nes
test_roms/cpu/instr/07-ind_x.nes
test_roms/cpu/instr/08-ind_y.nes
test_roms/cpu/instr/09-branches.nes
test_roms/cpu/instr/10-stack.nes
test_roms/cpu/instr/11-jmp_jsr.nes
test_roms/cpu/instr/12-rts.nes
test_roms/cpu/instr/13-rti.nes
test_roms/cpu/instr/14-brk.nes
test_roms/cpu/instr/15-special.nes
test_roms/cpu/instr_misc.nes
test_roms/cpu/instr_timing.nes
test_roms/cpu/interrupts/1-cli_latency.nes
test_roms/cpu/interrupts/2-nmi_and_brk.nes
test_roms/cpu/interrupts/3-nmi_and_irq.nes
test_roms/cpu/interrupts/5-branch_delays_irq.nes
test_roms/cpu/nestest.nes
test_roms/cpu/overclock.nes
test_roms/cpu/ram_after_reset.nes
test_roms/cpu/registers_after_reset.nes

## APU ============================================================================================
# These tests have a white garbled background with randomized RAM
test_roms/apu/01.len_ctr.nes
test_roms/apu/02.len_table.nes
test_roms/apu/03.irq_flag.nes
test_roms/apu/04.clock_jitter.nes
test_roms/apu/08.irq_timing.nes

test_roms/apu/dmc/buffer_retained.nes
test_roms/apu/dmc/latency.nes
test_roms/apu/dmc/status.nes
test_roms/apu/dmc/status_irq.nes
test_roms/apu/test_1.nes
test_roms/apu/test_2.nes
test_roms/apu/test_5.nes
test_roms/apu/test_6.nes

# Audio tests - Skip for now
# test_roms/apu/apu_env.nes
# test_roms/apu/dmc/dmc.nes
# test_roms/apu/dmc/dmc_pitch.nes
# test_roms/apu/lin_ctr.nes
# test_roms/apu/noise.nes
# test_roms/apu/noise_pitch.nes
# test_roms/apu/phase_reset.nes
# test_roms/apu/square.nes
# test_roms/apu/square_pitch.nes
# test_roms/apu/sweep_cutoff.nes
# test_roms/apu/sweep_sub.nes
# test_roms/apu/triangle.nes
# test_roms/apu/triangle_pitch.nes
# test_roms/apu/volumes.nes

## PPU ============================================================================================
test_roms/ppu/oam_read.nes
test_roms/ppu/oam_stress.nes
test_roms/ppu/oamtest3.nes # Not really sure what this tests
test_roms/ppu/open_bus.nes
test_roms/ppu/palette.nes
test_roms/ppu/palette_ram.nes
test_roms/ppu/read_buffer.nes
test_roms/ppu/scanline.nes
test_roms/ppu/sprite_ram.nes
test_roms/ppu/sprite_hit/01-basics.nes
test_roms/ppu/sprite_hit/02-alignment.nes
test_roms/ppu/sprite_hit/03-corners.nes
test_roms/ppu/sprite_hit/04-flip.nes
test_roms/ppu/sprite_hit/05-left_clip.nes
test_roms/ppu/sprite_hit/06-right_edge.nes
test_roms/ppu/sprite_hit/07-screen_bottom.nes
test_roms/ppu/sprite_hit/08-double_height.nes
test_roms/ppu/sprite_hit/10-timing_order.nes
test_roms/ppu/sprite_overflow/1.Basics.nes
test_roms/ppu/sprite_overflow/2.Details.nes
test_roms/ppu/sprite_overflow/5.Emulator.nes
test_roms/ppu/vbl_clear_time.nes
test_roms/ppu/vbl_nmi/01-vbl_basics.nes
test_roms/ppu/vbl_nmi/02-vbl_set_time.nes
test_roms/ppu/vbl_nmi/03-vbl_clear_time.nes
test_roms/ppu/vbl_nmi/04-nmi_control.nes
test_roms/ppu/vbl_nmi/05-nmi_timing.nes
test_roms/ppu/vbl_nmi/06-suppression.nes
test_roms/ppu/vbl_nmi/07-nmi_on_timing.nes
test_roms/ppu/vbl_nmi/08-nmi_off_timing.nes
test_roms/ppu/vbl_nmi/09-even_odd_frames.nes
test_roms/ppu/vbl_nmi_timing/1.frame_basics.nes
test_roms/ppu/vbl_nmi_timing/2.vbl_timing.nes
test_roms/ppu/vbl_nmi_timing/3.even_odd_frames.nes
test_roms/ppu/vbl_nmi_timing/4.vbl_clear_timing.nes
test_roms/ppu/vbl_nmi_timing/5.nmi_suppression.nes
test_roms/ppu/vbl_nmi_timing/6.nmi_disable.nes
test_roms/ppu/vbl_nmi_timing/7.nmi_timing.nes
test_roms/ppu/vram_access.nes

# Video
test_roms/ppu/240pee.nes
test_roms/ppu/color.nes
test_roms/ppu/ntsc_torture.nes
test_roms/ppu/tv.nes

## MAPPERS ========================================================================================
test_roms/mapper/mmc3/1.Clocking.nes
test_roms/mapper/mmc3/2.Details.nes
test_roms/mapper/mmc3/3.A12_clocking.nes
test_roms/mapper/mmc3/4.Scanline_timing.nes
test_roms/mapper/mmc3/6.MMC3_rev_B.nes
# test_roms/mapper/mmc3/5.MMC3_rev_A.nes # Can only pass rev_A or rev_B at the same time. Passes rev_B
test_roms/mapper/mmc3/mmc3bigchrram.nes

)

trap ctrl_c INT

function ctrl_c() {
    echo "** Trapped CTRL-C...Exiting"
    exit
}

for test in ${TESTS[*]}; do
    target/release/tetanes --speed 4 --consistent_ram $test
done

