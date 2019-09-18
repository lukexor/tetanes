cargo build
TESTS=(
## CPU ============================================================================================
tests/cpu/all_instrs.nes # 03-immediate 3/16 (ASR, ARR, AXS)
tests/cpu/flag_concurrency.nes # Timing doesn't match 29823 (got 35000) table should match OpenEMU results
tests/cpu/interrupts.nes # IRQ when $4017 == $00 1-cli_latency #3 1/5

# Not critical to emulate
tests/cpu/exec_space_apu.nes # Mysteriously landed at $4023 (should return open bus, but mmc5 uses that now)
tests/cpu/instr_misc.nes # ROL abs 03-dummy_reads #9 3/4

# Passes but maybe somethings wrong?
tests/cpu/instr_timing.nes # passes but calls ahx(), xaa(), shy(), shx()

## APU ============================================================================================
tests/apu/03.irq_flag.nes # $04
tests/apu/04.clock_jitter.nes # $03
tests/apu/05.len_timing_mode0.nes # $03
tests/apu/06.len_timing_mode1.nes # $03
tests/apu/07.irq_flag_timing.nes # $03
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
# All of these seem to fail on regular EMUs that work fine - I think the way they check for IRQs
# comes too late with most emulators
# tests/mapper/mmc3/1-clocking.nes # Should decrement when A12 is tolgged #3
# tests/mapper/mmc3/2-details.nes # Counter isn't working with 255 #2
# tests/mapper/mmc3/3-A12_clocking.nes # Should be clocked when A12 0->1 #4
# tests/mapper/mmc3/4-scanline_timing.nes # Scanline 0 IRQ should occur sooner #3
# tests/mapper/mmc3/5-MMC3.nes # Should reload and set IRQ every clock #2
# tests/mapper/mmc3/6-MMC3_alt.nes # Shouldnt IRQ when reload is 0 #2
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

