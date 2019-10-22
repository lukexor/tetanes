cargo build
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
tests/cpu/instr/07-ind_x.nes
tests/cpu/instr/08-ind_y.nes
tests/cpu/instr/09-branches.nes
tests/cpu/instr/10-stack.nes
tests/cpu/instr/11-jmp_jsr.nes
tests/cpu/instr/12-rts.nes
tests/cpu/instr/13-rti.nes
tests/cpu/instr/14-brk.nes
tests/cpu/instr/15-special.nes
tests/cpu/instr_timing.nes
tests/cpu/nestest.nes
tests/cpu/overclock.nes
tests/cpu/ram_after_reset.nes
tests/cpu/registers_after_reset.nes

## APU ============================================================================================
tests/apu/01.len_ctr.nes
tests/apu/02.len_table.nes
tests/apu/03.irq_flag.nes
tests/apu/04.clock_jitter.nes
tests/apu/08.irq_timing.nes #02

tests/apu/test_1.nes
tests/apu/test_2.nes
tests/apu/test_5.nes
tests/apu/test_6.nes

## APU Sound tests ================================================================================
# Skip for now
# tests/apu/apu_env.nes
# tests/apu/dmc.nes
# tests/apu/dmc_pitch.nes
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
tests/ppu/240pee.nes
tests/ppu/color.nes
tests/ppu/ntsc_torture.nes
tests/ppu/oam_read.nes
tests/ppu/oam_stress.nes
tests/ppu/open_bus.nes
tests/ppu/palette.nes
tests/ppu/palette_ram.nes
tests/ppu/read_buffer.nes
tests/ppu/scanline.nes
tests/ppu/sprite_ram.nes
tests/ppu/tv.nes
tests/ppu/vbl_clear_time.nes
tests/ppu/vbl_nmi_timing/1.frame_basics.nes
tests/ppu/vbl_nmi_timing/4.vbl_clear_timing.nes
tests/ppu/vram_access.nes

## MAPPERS ========================================================================================
tests/mapper/mmc3/1.Clocking.nes
tests/mapper/mmc3/2.Details.nes
tests/mapper/mmc3/3.A12_clocking.nes
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
    target/debug/rustynes $test
done

