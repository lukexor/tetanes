cargo build
TESTS=(
## CPU ============================================================================================
tests/cpu/all_instrs.nes # 9C SYA, 9E SXA - 7 of 16
tests/cpu/flag_concurrency.nes # Timing doesn't match 29823 (got 35000) table should match OpenEMU results
tests/cpu/interrupts.nes # IRQ when $4017 == $00 1-cli_latency #3 1/5

# Not critical to emulate
tests/cpu/exec_space_apu.nes # Mysteriously landed at $4023 (should return open bus, but mmc5 uses that now)
tests/cpu/instr_misc.nes # 04-dummy_reads_apu #2 4 of 4

## APU ============================================================================================
tests/apu/03.irq_flag.nes # $04
tests/apu/04.clock_jitter.nes # $03
tests/apu/05.len_timing_mode0.nes # $03
tests/apu/06.len_timing_mode1.nes # $03
tests/apu/07.irq_flag_timing.nes # $03
tests/apu/08.irq_timing.nes # $03
tests/apu/09.reset_timing.nes # $04
tests/apu/10.len_halt_timing.nes # $03
tests/apu/11.len_reload_timing.nes # $03
tests/apu/test.nes # writing $00 to $4017 shouldn't clock length immediately, 1-len_ctr #5 1/8
tests/apu/test_3.nes # failed
tests/apu/test_4.nes # failed
tests/apu/test_7.nes # failed
tests/apu/test_8.nes # failed
tests/apu/test_9.nes # failed
tests/apu/test_10.nes # failed

## PPU ============================================================================================
tests/ppu/sprdma_and_dmc_dma.nes # Supposed to print a table
tests/ppu/sprdma_and_dmc_dma_512.nes # Supposed to print a table
tests/ppu/sprite_hit.nes # flag set too soon for lower-left corner 09-timing #7 9/10
tests/ppu/sprite_overflow.nes # flag cleared too late at end of VBL 03-timing #4 3/5
tests/ppu/vbl_nmi.nes # 02-vbl_set_time 2/10
tests/ppu/vbl_nmi_timing/2.vbl_timing.nes # #8
tests/ppu/vbl_nmi_timing/3.even_odd_frames.nes # #3
tests/ppu/vbl_nmi_timing/5.nmi_suppression.nes #4
tests/ppu/vbl_nmi_timing/6.nmi_disable.nes # #3
tests/ppu/vbl_nmi_timing/7.nmi_timing.nes # #7

# Nice to have
tests/ppu/nmi_sync_ntsc.nes # Not sure what it tests
tests/ppu/ntsc_torture.nes # No NTSC raster effects
tests/ppu/palette.nes # Doesn't support emphasis or grayscale
tests/ppu/tv.nes # Passes ratio, but not chroma/luma

## MAPPERS ========================================================================================
tests/mapper/mmc3/4.Scanline_timing.nes # Failed #2 - scanline 0 is too soon
tests/mapper/mmc3/5.MMC3_rev_A.nes # Can only pass rev_A or rev_B at the same time. Passes rev_B

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

