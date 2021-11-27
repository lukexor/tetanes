# Some tests rely on deterministic RAM state
cargo build --release --features no-randomize-ram

# Count: 20

TESTS=(
## CPU ============================================================================================
test_roms/cpu/interrupts/4-irq_and_dma.nes # ??

## APU ============================================================================================
test_roms/apu/05.len_timing_mode0.nes # $04
test_roms/apu/06.len_timing_mode1.nes # $05
test_roms/apu/07.irq_flag_timing.nes # $04
test_roms/apu/09.reset_timing.nes # $04
test_roms/apu/10.len_halt_timing.nes # $03
test_roms/apu/11.len_reload_timing.nes # $04
test_roms/apu/dpcmletterbox.nes # Jitters too much
test_roms/apu/test.nes # Channel: 2, Problem with length counter loador $4015, 1-len-ctr #2 1 of 8
test_roms/apu/test_3.nes # failed
test_roms/apu/test_4.nes # failed
test_roms/apu/test_7.nes # failed
test_roms/apu/test_8.nes # failed
test_roms/apu/test_9.nes # failed
test_roms/apu/test_10.nes # failed

## PPU ============================================================================================
test_roms/ppu/sprdma_and_dmc_dma.nes # Supposed to print a table and instead just makes pitch noise
test_roms/ppu/sprdma_and_dmc_dma_512.nes # Supposed to print a table and instead just makes pitch noise
test_roms/ppu/sprite_hit/09-timing.nes # Flag set too soon for upper-right corner #5
test_roms/ppu/sprite_overflow/3.Timing.nes # Failed #5
test_roms/ppu/sprite_overflow/4.Obscure.nes # Failed #2
test_roms/ppu/vbl_nmi/10-even_odd_timing.nes # Failed #3
)

trap ctrl_c INT

function ctrl_c() {
    echo "** Trapped CTRL-C...Exiting"
    exit
}

for test in ${TESTS[*]}; do
    target/release/tetanes --speed 4 $test
done

