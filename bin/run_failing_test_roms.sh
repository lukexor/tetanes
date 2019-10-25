cargo build
TESTS=(
tests/ppu/vbl_nmi/10-even_odd_timing.nes

# ## CPU ============================================================================================
# tests/cpu/instr/06-abs_xy.nes # 9C, 9E
# tests/cpu/interrupts/2-nmi_and_brk.nes # ??
# tests/cpu/interrupts/3-nmi_and_irq.nes # ??
# tests/cpu/interrupts/4-irq_and_dma.nes # ??

# ## APU ============================================================================================
# tests/apu/05.len_timing_mode0.nes # $04
# tests/apu/06.len_timing_mode1.nes # $05
# tests/apu/07.irq_flag_timing.nes # $04
# tests/apu/09.reset_timing.nes # $04
# tests/apu/10.len_halt_timing.nes # $03
# tests/apu/11.len_reload_timing.nes # $04
# tests/apu/test.nes # writing $00 to $4017 shouldn't clock length immediately, 1-len_ctr #5 1/8
# tests/apu/test_3.nes # failed
# tests/apu/test_4.nes # failed
# tests/apu/test_7.nes # failed
# tests/apu/test_8.nes # failed
# tests/apu/test_9.nes # failed
# tests/apu/test_10.nes # failed

# ## PPU ============================================================================================
# tests/ppu/scanline.nes # flickers
# tests/ppu/sprdma_and_dmc_dma.nes # Supposed to print a table
# tests/ppu/sprdma_and_dmc_dma_512.nes # Supposed to print a table
# tests/ppu/sprite_hit.nes # flag cleared too late at end of VBL 09-timing #10 9/10
# tests/ppu/sprite_overflow.nes # PPU VBL timing is wrong 03-timing #3 3/5

# # Nice to have
# tests/ppu/nmi_sync_ntsc.nes # Not sure what it tests

# ## MAPPERS ========================================================================================
# tests/mapper/mmc3/4.Scanline_timing.nes # Failed #7
# tests/mapper/mmc3/5.MMC3_rev_A.nes # Can only pass rev_A or rev_B at the same time. Passes rev_B

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

