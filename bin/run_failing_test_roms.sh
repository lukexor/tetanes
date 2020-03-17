# Some tests rely on deterministic RAM state
cargo build --features no-randomize-ram
TESTS=(
## CPU ============================================================================================
tests/cpu/interrupts/4-irq_and_dma.nes # ??

## APU ============================================================================================
tests/apu/05.len_timing_mode0.nes # $04
tests/apu/06.len_timing_mode1.nes # $05
tests/apu/07.irq_flag_timing.nes # $04
tests/apu/09.reset_timing.nes # $04
tests/apu/10.len_halt_timing.nes # $03
tests/apu/11.len_reload_timing.nes # $04
tests/apu/dpcmletterbox.nes
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
tests/ppu/sprite_hit/09-timing.nes # Flag set too soon for upper-right corner #05
tests/ppu/sprite_overflow/3.Timing.nes # Failed #05
tests/ppu/sprite_overflow/4.Obscure.nes # Failed #02

## MAPPERS ========================================================================================
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

