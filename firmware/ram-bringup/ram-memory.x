/* ESP32-S3 internal-memory-only layout for espflash --ram. */
MEMORY
{
  vectors_seg ( RX ) : ORIGIN = 0x40370000 + 0x400, len = 0x400
  iram_seg ( RX ) : ORIGIN = 0x40370800, len = 0x5D000
  dram_seg ( RW ) : ORIGIN = 0x3FC88000, len = 0x55000
  /* The RAM image has no second-stage app loader reserving the upper DRAM
     window, so the bring-up data/stack region may use the full internal
     range through 0x3FD00000. */
  dram2_seg ( RW ) : ORIGIN = 0x3FCDD000, len = 0x23000
  irom_seg ( RX ) : ORIGIN = 0x40370800, len = 0x52000
  drom_seg ( R ) : ORIGIN = 0x3FC88000, len = 0x55000
  rtc_fast_seg ( RWX ) : ORIGIN = 0x600FE000, len = 8K
  rtc_slow_seg ( RW ) : ORIGIN = 0x50000000, len = 8K
}
