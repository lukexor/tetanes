cargo build --profile dev-opt

TESTS=(
## APU ============================================================================================
test_roms/apu/05.len_timing_mode0.nes # $04
test_roms/apu/06.len_timing_mode1.nes # $05
test_roms/apu/07.irq_flag_timing.nes # $04
test_roms/apu/09.reset_timing.nes # $04
test_roms/apu/10.len_halt_timing.nes # $03
test_roms/apu/11.len_reload_timing.nes # $04
test_roms/apu/dpcmletterbox.nes # Jitters and should be rendered at lower scanline
test_roms/apu/test.nes # Channel: 0 second length of mode 0 is too soon, 5-len-timing #4 5 of 8
test_roms/apu/test_3.nes # failed
test_roms/apu/test_4.nes # failed
test_roms/apu/test_7.nes # failed
test_roms/apu/test_8.nes # failed
test_roms/apu/test_9.nes # failed
test_roms/apu/test_10.nes # failed

## PPU ============================================================================================
test_roms/ppu/sprdma_and_dmc_dma.nes # 1791 EEDCF180 Failed
test_roms/ppu/sprdma_and_dmc_dma_512.nes # 1791 EEDCF180 Failed
test_roms/ppu/sprite_hit/09-timing.nes # Flag set too soon for upper-right corner #5
test_roms/ppu/sprite_overflow/3.Timing.nes # Failed #5
test_roms/ppu/sprite_overflow/4.Obscure.nes # Failed #2
test_roms/ppu/vbl_nmi/10-even_odd_timing.nes # Clock is skipped too late relative to enabling BG Failed #3
)

trap ctrl_c INT

function ctrl_c() {
    echo "** Trapped CTRL-C...Exiting"
    exit
}

for test in ${TESTS[*]}; do
    target/dev-opt/tetanes --speed 4 --consistent_ram $test
done

